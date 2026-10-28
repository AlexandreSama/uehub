//! Extraire le moteur pendant qu'il se télécharge : le zip ne passe jamais par le disque.
//!
//! 1. D'abord l'index : la fin du zip, quelques Mo, par une requête Range. Il donne chaque
//!    fichier, avec son nom, ses tailles, son CRC, ses droits et l'endroit où il commence.
//! 2. Puis le zip arrive d'un bloc, à partir du premier fichier qui reste à écrire. Un fil à
//!    part décompresse chaque fichier au passage, et vérifie son CRC.
//! 3. Tous les 256 Mo écrits, tout part sur le disque (syncfs), puis le journal note le
//!    fichier suivant. Une coupure reprend au fichier en cours ; un plantage, au dernier point.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bytes::Bytes;
use reqwest::StatusCode;
use reqwest::header::RANGE;
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive};

use super::{
    Downloaded, InstallProgress, REPORT_EVERY, StopOnDrop, common_root, file_mode, stays_inside,
    target_of, zip_error,
};
use crate::{CoreError, Result, t};

/// Le journal d'une installation en cours, dans le dossier du moteur.
pub(super) const JOURNAL: &str = ".uehub-install";
/// L'index du zip (sa fin), gardé pour les reprises.
pub(super) const INDEX: &str = ".uehub-index";
/// Tous les 256 Mo écrits, un point d'enregistrement.
const CHECKPOINT: u64 = 256 << 20;
/// La fin de l'index fait 22 octets, suivis d'un commentaire de 64 Ko au plus.
const PROBE: u64 = 22 + 65_535;
/// Les morceaux reçus attendent là que le fil d'écriture les prenne : quelques Mo.
const QUEUE: usize = 256;
/// Un lien symbolique tient en quelques octets : au-delà, l'entrée est suspecte.
const MAX_LINK: u64 = 4096;

/// Où en est l'installation. Remplacé d'un coup à chaque point d'enregistrement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Journal {
    /// La taille du zip et le début de son index : une reprise vérifie que c'est le même
    pub size: u64,
    pub index_start: u64,
    /// Le nombre d'entrées du zip
    pub files: usize,
    /// La prochaine entrée à écrire, dans l'ordre du zip, et l'octet où elle commence
    pub next: usize,
    pub offset: u64,
    /// Octets déjà écrits (décompressés) : ce qui reste à prévoir sur le disque
    pub written: u64,
}

/// Le journal de `dir`, s'il y a une installation en cours.
pub(super) fn read_journal(dir: &Path) -> Option<Journal> {
    let text = fs::read(dir.join(JOURNAL)).ok()?;
    serde_json::from_slice(&text).ok()
}

/// Écrit le journal à côté, puis le met en place d'un coup : un plantage laisse l'ancien
/// ou le nouveau, jamais un mélange.
pub(super) fn write_journal(dir: &Path, journal: &Journal) -> io::Result<()> {
    let tmp = dir.join(format!("{JOURNAL}.tmp"));
    fs::write(&tmp, serde_json::to_vec(journal)?)?;
    fs::rename(&tmp, dir.join(JOURNAL))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Method {
    Stored,
    Deflated,
    Deflate64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    File,
    Dir,
    Link,
}

/// Une entrée du zip, telle que l'index la décrit.
struct Entry {
    /// Son chemin dans le zip ; `None` s'il sortirait du dossier (il est alors sauté)
    path: Option<PathBuf>,
    /// L'octet où commence son en-tête local
    start: u64,
    compressed: u64,
    size: u64,
    crc: u32,
    method: Method,
    kind: Kind,
    mode: Option<u32>,
}

/// L'index du zip : ses entrées dans l'ordre où elles arrivent.
pub(super) struct Index {
    entries: Vec<Entry>,
    /// Le dossier de tête à retirer, s'il y en a un
    root: Option<PathBuf>,
    /// Où commence l'index dans le zip : les données s'arrêtent là
    start: u64,
}

impl Index {
    /// La place qu'il faut encore, à partir de l'entrée `next` : chaque fichier arrondi au
    /// bloc de 4 Ko qu'il occupe au moins sur le disque.
    pub(super) fn remaining_space(&self, next: usize) -> u64 {
        self.entries[next.min(self.entries.len())..]
            .iter()
            .filter(|e| e.kind == Kind::File && e.path.is_some())
            .map(|e| e.size.div_ceil(4096) * 4096)
            .sum()
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Lit l'index : `tail` est la fin du zip, à partir de l'octet `start`.
pub(super) fn parse(tail: Vec<u8>, size: u64, start: u64) -> Result<Index> {
    // Le zip commence au début du fichier. Sans le dire, le crate cherche où il commence
    // en lisant tout le fichier : 40 Go de zéros pour le zip d'Epic.
    let config = zip::read::Config {
        archive_offset: zip::read::ArchiveOffset::Known(0),
    };
    let archive = ZipArchive::with_config(
        config,
        Sparse {
            size,
            start,
            tail,
            pos: 0,
        },
    )
    .map_err(zip_error)?;
    let root = common_root(&archive)?;
    let mut entries = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let e = archive.by_index_data(i).map_err(zip_error)?;
        let name = e.name().map_err(zip_error)?.into_owned();
        let unreadable = |what: String| {
            CoreError::Download(t!("errors.zip", detail = format!("{name}: {what}")))
        };
        if e.encrypted() {
            return Err(unreadable("encrypted".into()));
        }
        let method = match e.compression() {
            CompressionMethod::Stored => Method::Stored,
            CompressionMethod::Deflated => Method::Deflated,
            CompressionMethod::Deflate64 => Method::Deflate64,
            other => return Err(unreadable(format!("unsupported compression ({other})"))),
        };
        let kind = if e.is_symlink() {
            Kind::Link
        } else if e.is_dir() {
            Kind::Dir
        } else {
            Kind::File
        };
        entries.push(Entry {
            path: e.enclosed_name(),
            start: e.header_start(),
            compressed: e.compressed_size(),
            size: e.size(),
            crc: e.crc32(),
            method,
            kind,
            mode: e.unix_mode(),
        });
    }
    entries.sort_by_key(|e| e.start);
    // Chaque entrée finit avant la suivante, et la dernière avant l'index. Sinon le zip est
    // abîmé, ou piégé : deux entrées qui partagent les mêmes octets (une « zip bomb »).
    let ends: Vec<u64> = entries
        .iter()
        .skip(1)
        .map(|e| e.start)
        .chain([start])
        .collect();
    let overlap = entries
        .iter()
        .zip(ends)
        .any(|(e, next)| e.start.saturating_add(30 + e.compressed) > next);
    if overlap || start > size {
        return Err(CoreError::Download(t!(
            "errors.zip",
            detail = "entries overlap"
        )));
    }
    Ok(Index {
        entries,
        root,
        start,
    })
}

/// Le zip vu à travers sa seule fin : avant `start`, rien que des zéros. Le crate zip y lit
/// l'index comme dans le vrai fichier, sans jamais toucher aux données.
struct Sparse {
    size: u64,
    start: u64,
    tail: Vec<u8>,
    pos: u64,
}

impl Read for Sparse {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.size.saturating_sub(self.pos) as usize);
        #[cfg(test)]
        SPARSE_READ.with(|read| read.set(read.get() + n as u64));
        let (out, end) = (&mut buf[..n], self.pos + n as u64);
        // La partie avant l'index : des zéros. La suite : l'index lui-même.
        let zeros = (self.start.saturating_sub(self.pos) as usize).min(n);
        out[..zeros].fill(0);
        if end > self.start {
            let from = (self.pos.max(self.start) - self.start) as usize;
            let to = ((end - self.start) as usize).min(self.tail.len());
            let copied = to.saturating_sub(from);
            out[zeros..zeros + copied].copy_from_slice(&self.tail[from..from + copied]);
            out[zeros + copied..].fill(0);
        }
        self.pos = end;
        Ok(n)
    }
}

#[cfg(test)]
thread_local! {
    /// Ce que le crate zip a lu dans `Sparse`, pour vérifier qu'il ne lit que l'index.
    pub(super) static SPARSE_READ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl Seek for Sparse {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.size.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        self.pos = pos.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek"))?;
        Ok(self.pos)
    }
}

/// Où commence l'index, d'après la fin du zip (`tail` commence à l'octet `from`).
pub(super) fn index_start(tail: &[u8], from: u64) -> Result<u64> {
    let bad = || CoreError::Download(t!("errors.zip", detail = "no end of central directory"));
    let u32_at = |i: usize| u32::from_le_bytes(tail[i..i + 4].try_into().unwrap());
    let u64_at = |i: usize| u64::from_le_bytes(tail[i..i + 8].try_into().unwrap());
    // La fin de l'index : sa signature, et au moins ses 22 octets derrière.
    let end = tail
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .filter(|&at| tail.len() - at >= 22)
        .ok_or_else(bad)?;
    let start = u32_at(end + 16);
    if start != u32::MAX {
        return Ok(start.into());
    }
    // ZIP64 (plus de 4 Go) : un localisateur, juste avant, donne où est l'enregistrement
    // ZIP64, qui donne le début de l'index.
    let locator = end
        .checked_sub(20)
        .filter(|&at| &tail[at..at + 4] == b"PK\x06\x07")
        .ok_or_else(bad)?;
    let record = u64_at(locator + 8)
        .checked_sub(from)
        .map(|at| at as usize)
        .filter(|&at| at + 56 <= tail.len() && &tail[at..at + 4] == b"PK\x06\x06")
        .ok_or_else(bad)?;
    Ok(u64_at(record + 48))
}

/// Ce que le serveur répond à une requête « à partir de tel octet ».
enum Answer {
    /// 206 : la suite, à partir de l'octet demandé
    Partial(reqwest::Response),
    /// 200 : tout le fichier, depuis le début
    Whole(reqwest::Response),
    Expired,
    Interrupted,
}

async fn request(http: &reqwest::Client, url: &str, from: u64) -> Result<Answer> {
    let response = match http
        .get(url)
        .header(RANGE, format!("bytes={from}-"))
        .send()
        .await
    {
        Ok(response) => response,
        // Réseau coupé ou serveur muet avant même de répondre : on réessaiera.
        Err(e) if e.is_connect() || e.is_timeout() => {
            log::warn!("engine zip: {}, will resume", CoreError::from(e).detail());
            return Ok(Answer::Interrupted);
        }
        Err(e) => return Err(e.into()),
    };
    Ok(match response.status() {
        StatusCode::PARTIAL_CONTENT => Answer::Partial(response),
        StatusCode::OK => Answer::Whole(response),
        // Une adresse signée périmée donne 403.
        StatusCode::FORBIDDEN => {
            log::info!("engine zip: the address expired, asking for a new one");
            Answer::Expired
        }
        status => {
            return Err(CoreError::Download(t!(
                "errors.zip_status",
                status = status
            )));
        }
    })
}

/// Comment s'est fini l'essai de lire l'index.
pub(super) enum Fetched {
    /// La fin du zip, à partir de l'octet donné
    Tail(Vec<u8>, u64),
    Expired,
    Interrupted,
    /// Le serveur ignore Range : il n'envoie que le fichier entier
    NoRanges,
}

/// Les octets du zip à partir de `from`, jusqu'au bout.
async fn read_from(http: &reqwest::Client, url: &str, from: u64, size: u64) -> Result<Fetched> {
    let response = match request(http, url, from).await? {
        Answer::Partial(response) => response,
        Answer::Whole(response) if from == 0 => response,
        Answer::Whole(_) => return Ok(Fetched::NoRanges),
        Answer::Expired => return Ok(Fetched::Expired),
        Answer::Interrupted => return Ok(Fetched::Interrupted),
    };
    match response.bytes().await {
        Ok(bytes) if bytes.len() as u64 == size - from => Ok(Fetched::Tail(bytes.into(), from)),
        Ok(_) => Ok(Fetched::Interrupted),
        Err(e) => {
            log::warn!("engine zip: {}, will resume", CoreError::from(e).detail());
            Ok(Fetched::Interrupted)
        }
    }
}

/// L'index du zip, en une ou deux requêtes : les 64 derniers Ko d'abord, qui disent où il
/// commence, puis le reste s'il est plus long.
pub(super) async fn fetch_index(http: &reqwest::Client, url: &str, size: u64) -> Result<Fetched> {
    let from = size.saturating_sub(PROBE);
    let probe = match read_from(http, url, from, size).await? {
        Fetched::Tail(bytes, _) => bytes,
        other => return Ok(other),
    };
    let start = index_start(&probe, from)?;
    if start >= from {
        return Ok(Fetched::Tail(
            probe[(start - from) as usize..].to_vec(),
            start,
        ));
    }
    read_from(http, url, start, size).await
}

/// Une installation qui commence : l'index est gardé, puis le journal écrit.
pub(super) fn begin(dir: &Path, size: u64, tail: Vec<u8>, start: u64) -> Result<(Index, Journal)> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(INDEX), &tail)?;
    let index = parse(tail, size, start)?;
    log_index(&index);
    let journal = Journal {
        size,
        index_start: start,
        files: index.len(),
        next: 0,
        offset: index.entries.first().map_or(start, |e| e.start),
        written: 0,
    };
    write_journal(dir, &journal)?;
    Ok((index, journal))
}

/// Une installation en cours : son journal et son index, s'ils correspondent à ce zip.
pub(super) fn resume(dir: &Path, size: u64) -> Option<(Index, Journal)> {
    let journal = read_journal(dir).filter(|j| j.size == size)?;
    let tail = fs::read(dir.join(INDEX)).ok()?;
    match parse(tail, size, journal.index_start) {
        Ok(index) if index.len() == journal.files => Some((index, journal)),
        _ => None,
    }
}

/// Le portrait du zip, dans le journal de uehub : de quoi savoir à quoi ressemblent les
/// vrais zips d'Epic (nombre de fichiers, le plus gros, les méthodes de compression).
fn log_index(index: &Index) {
    let files = index.entries.iter().filter(|e| e.kind == Kind::File);
    let total: u64 = files.clone().map(|e| e.size).sum();
    let largest = files.clone().map(|e| e.compressed).max().unwrap_or(0);
    let count = |m: Method| index.entries.iter().filter(|e| e.method == m).count();
    log::info!(
        "engine zip index: {} entries, {} bytes once extracted, largest file {} bytes compressed, \
         stored {}, deflate {}, deflate64 {}",
        index.len(),
        total,
        largest,
        count(Method::Stored),
        count(Method::Deflated),
        count(Method::Deflate64)
    );
}

/// Télécharge le zip à partir de la prochaine entrée et l'écrit dans `dir` au passage.
pub(super) async fn stream(
    http: &reqwest::Client,
    url: &str,
    dir: &Path,
    index: Index,
    journal: Journal,
    on_progress: &(dyn Fn(InstallProgress) + Send + Sync),
) -> Result<Downloaded> {
    let (size, from, end) = (journal.size, journal.offset, index.start);
    if journal.next >= index.len() {
        return Ok(Downloaded::Complete);
    }
    let (mut response, mut position) = match request(http, url, from).await? {
        Answer::Partial(response) => (response, from),
        // Tout le fichier : le début, déjà écrit, sera lu et jeté.
        Answer::Whole(response) => (response, 0),
        Answer::Expired => return Ok(Downloaded::Expired),
        Answer::Interrupted => return Ok(Downloaded::Interrupted),
    };
    on_progress(InstallProgress::Download {
        done: from,
        total: size,
    });

    // Le fil d'écriture lit ce que la boucle ci-dessous reçoit. La file est bornée : si le
    // disque est plus lent que le réseau, la réception attend.
    let (sender, receiver) = tokio::sync::mpsc::channel(QUEUE);
    let stop = Arc::new(AtomicBool::new(false));
    // Si cette tâche est abandonnée (pause), ce garde arrête l'écriture au fichier suivant.
    let _stop_on_drop = StopOnDrop(stop.clone());
    let input = Input::new(receiver, position);
    let target = dir.to_path_buf();
    let worker =
        tokio::task::spawn_blocking(move || write_entries(&index, &target, journal, input, &stop));

    let mut reported = Instant::now();
    // Au-delà de `end`, c'est l'index : on l'a déjà.
    while position < end {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                position += chunk.len() as u64;
                // Le fil d'écriture s'est arrêté (une erreur) : inutile de recevoir la suite.
                if sender.send(chunk).await.is_err() {
                    break;
                }
                if reported.elapsed() >= REPORT_EVERY {
                    on_progress(InstallProgress::Download {
                        done: position.clamp(from, size),
                        total: size,
                    });
                    reported = Instant::now();
                }
            }
            Ok(None) => break,
            // Coupure réseau : le fil d'écriture finit ce qu'il a reçu, on reprendra.
            Err(e) => {
                log::warn!("engine zip: {}, will resume", CoreError::from(e).detail());
                break;
            }
        }
    }
    drop(sender);
    drop(response);
    let ended = worker
        .await
        .map_err(|e| CoreError::Download(t!("errors.extraction", detail = e)))??;
    Ok(match ended {
        Ended::Done => {
            on_progress(InstallProgress::Download {
                done: size,
                total: size,
            });
            Downloaded::Complete
        }
        Ended::Cut => Downloaded::Interrupted,
    })
}

/// Comment s'est arrêté le fil d'écriture.
#[derive(Debug, PartialEq)]
enum Ended {
    /// Toutes les entrées sont écrites
    Done,
    /// Le flux s'est arrêté avant (coupure, pause) : on reprendra à l'entrée en cours
    Cut,
}

/// Le zip tel qu'il arrive du réseau, morceau par morceau. `position` est l'octet du zip
/// qui sera lu ensuite.
struct Input {
    receiver: tokio::sync::mpsc::Receiver<Bytes>,
    chunk: Bytes,
    position: u64,
    /// Le flux s'est arrêté : il n'arrivera plus rien
    ended: bool,
}

impl Input {
    fn new(receiver: tokio::sync::mpsc::Receiver<Bytes>, position: u64) -> Self {
        Self {
            receiver,
            chunk: Bytes::new(),
            position,
            ended: false,
        }
    }

    /// Attend un morceau non vide ; `false` si le flux est fini.
    fn fill(&mut self) -> bool {
        while self.chunk.is_empty() {
            match self.receiver.blocking_recv() {
                Some(chunk) => self.chunk = chunk,
                None => {
                    self.ended = true;
                    return false;
                }
            }
        }
        true
    }

    /// Jette tout ce qui précède `offset` (ce qui sépare deux entrées, ou le début du
    /// fichier quand le serveur l'envoie en entier).
    fn skip_to(&mut self, offset: u64) -> io::Result<()> {
        while self.position < offset {
            if !self.fill() {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let n = (offset - self.position).min(self.chunk.len() as u64) as usize;
            let _ = self.chunk.split_to(n);
            self.position += n as u64;
        }
        Ok(())
    }
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || !self.fill() {
            return Ok(0);
        }
        let n = buf.len().min(self.chunk.len());
        buf[..n].copy_from_slice(&self.chunk.split_to(n));
        self.position += n as u64;
        Ok(n)
    }
}

/// Écrit les entrées à partir de `journal.next`, jusqu'à la fin du zip ou du flux.
/// Bloquant : tourne sur un fil à part.
fn write_entries(
    index: &Index,
    dir: &Path,
    mut journal: Journal,
    mut input: Input,
    stop: &AtomicBool,
) -> Result<Ended> {
    let mut next = journal.next;
    let mut unsynced = 0;
    let ended = loop {
        if next == index.len() {
            break Ok(Ended::Done);
        }
        if stop.load(Ordering::Relaxed) {
            break Ok(Ended::Cut);
        }
        let entry = &index.entries[next];
        match write_entry(entry, index.root.as_deref(), dir, &mut input) {
            Ok(()) => {
                next += 1;
                unsynced += entry.size;
                if unsynced >= CHECKPOINT {
                    checkpoint(index, dir, &mut journal, next)?;
                    unsynced = 0;
                }
            }
            // Le flux s'est arrêté au milieu de cette entrée : ce n'est pas sa faute.
            Err(_) if input.ended => break Ok(Ended::Cut),
            Err(e) => break Err(e),
        }
    };
    // Ce qui est fini est enregistré, même après une erreur : la reprise repart d'ici.
    checkpoint(index, dir, &mut journal, next)?;
    ended
}

/// Envoie sur le disque tout ce qui est écrit, puis note dans le journal que les entrées
/// avant `next` sont faites. Dans cet ordre : le journal n'annonce jamais un fichier qui
/// pourrait se perdre dans une coupure de courant.
fn checkpoint(index: &Index, dir: &Path, journal: &mut Journal, next: usize) -> Result<()> {
    if next == journal.next {
        return Ok(());
    }
    rustix::fs::syncfs(File::open(dir)?).map_err(io::Error::from)?;
    let done = &index.entries[journal.next..next];
    journal.written += done.iter().map(|e| e.size).sum::<u64>();
    journal.next = next;
    journal.offset = index.entries.get(next).map_or(index.start, |e| e.start);
    write_journal(dir, journal)?;
    Ok(())
}

/// Lit une entrée dans le flux, et l'écrit : dossier, lien symbolique ou fichier.
fn write_entry(entry: &Entry, root: Option<&Path>, dir: &Path, input: &mut Input) -> Result<()> {
    // L'en-tête local : sa signature, puis le nom et le champ extra, qu'on saute.
    input.skip_to(entry.start)?;
    let mut fixed = [0u8; 30];
    input.read_exact(&mut fixed)?;
    if fixed[..4] != *b"PK\x03\x04" {
        return Err(CoreError::Download(t!(
            "errors.zip",
            detail = format!("no local header at byte {}", entry.start)
        )));
    }
    let name = u16::from_le_bytes([fixed[26], fixed[27]]);
    let extra = u16::from_le_bytes([fixed[28], fixed[29]]);
    input.skip_to(input.position + u64::from(name) + u64::from(extra))?;

    let mut data = Read::take(&mut *input, entry.compressed);
    let target = target_of(entry.path.clone(), root, dir);
    if let Some(target) = target {
        let relative = target.strip_prefix(dir).unwrap_or(&target).to_path_buf();
        match entry.kind {
            Kind::Dir => fs::create_dir_all(&target)?,
            Kind::Link => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                // Le contenu de l'entrée est la cible du lien.
                let mut link = String::new();
                decoder(entry.method, &mut data)
                    .take(MAX_LINK)
                    .read_to_string(&mut link)?;
                if stays_inside(&relative, Path::new(&link)) {
                    let _ = fs::remove_file(&target);
                    symlink(&link, &target)?;
                }
            }
            Kind::File => write_file(entry, &target, &relative, &mut data)?,
        }
    }
    // Ce qui reste de l'entrée (un chemin dangereux, sauté, ou un flux deflate qui finit
    // avant ses octets) est lu et jeté : le flux se retrouve au début de la suivante.
    io::copy(&mut data, &mut io::sink())?;
    Ok(())
}

/// Décompresse un fichier vers le disque, en vérifiant sa taille et son CRC. Un fichier
/// abîmé ou incomplet est effacé.
fn write_file(entry: &Entry, target: &Path, relative: &Path, data: &mut dyn Read) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = File::create(target)?;
    let written = copy_checked(&mut decoder(entry.method, data), &mut out);
    drop(out);
    match written {
        Ok((size, crc)) if size == entry.size && crc == entry.crc => {}
        Ok(_) => {
            let _ = fs::remove_file(target);
            return Err(CoreError::Download(t!(
                "errors.damaged",
                file = relative.display()
            )));
        }
        Err(e) => {
            let _ = fs::remove_file(target);
            return Err(CoreError::Download(t!(
                "errors.file",
                file = relative.display(),
                detail = e
            )));
        }
    }
    let mode = file_mode(entry.mode, relative);
    fs::set_permissions(target, fs::Permissions::from_mode(mode))?;
    Ok(())
}

/// Le décodeur de la méthode de compression de l'entrée.
fn decoder<'a>(method: Method, data: &'a mut dyn Read) -> Box<dyn Read + 'a> {
    match method {
        Method::Stored => Box::new(data),
        Method::Deflated => Box::new(flate2::read::DeflateDecoder::new(data)),
        Method::Deflate64 => Box::new(deflate64::Deflate64Decoder::new(data)),
    }
}

/// Copie `from` dans `to` par blocs de 256 Ko, et renvoie la taille et le CRC copiés.
fn copy_checked(from: &mut dyn Read, to: &mut File) -> io::Result<(u64, u32)> {
    use std::io::Write;
    let mut buf = vec![0u8; 256 << 10];
    let mut crc = crc32fast::Hasher::new();
    let mut size = 0;
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        crc.update(&buf[..n]);
        to.write_all(&buf[..n])?;
        size += n as u64;
    }
    Ok((size, crc.finalize()))
}
