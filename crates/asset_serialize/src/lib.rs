pub mod adr;
pub mod bvh;
pub mod cdt;
pub mod pack;

use walkdir::WalkDir;

use std::{
    collections::HashMap,
    future::Future,
    io::SeekFrom,
    num::TryFromIntError,
    path::{Path, PathBuf},
    string::FromUtf8Error,
};

use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, BufReader},
    task::JoinSet,
};

use crate::pack::Pack;

#[derive(Debug)]
pub enum ErrorKind {
    InvalidUtf8(FromUtf8Error),
    Io(tokio::io::Error),
    TryFromInt(TryFromIntError),
    UnknownDiscriminant(u64, &'static str),
    UnknownMagic(String),
}

impl From<FromUtf8Error> for ErrorKind {
    fn from(value: FromUtf8Error) -> Self {
        ErrorKind::InvalidUtf8(value)
    }
}

impl From<tokio::io::Error> for ErrorKind {
    fn from(value: tokio::io::Error) -> Self {
        ErrorKind::Io(value)
    }
}

impl From<TryFromIntError> for ErrorKind {
    fn from(value: TryFromIntError) -> Self {
        ErrorKind::TryFromInt(value)
    }
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: Option<u64>,
}

pub trait DeserializeAsset: Sized {
    fn deserialize<P: AsRef<Path> + Send>(
        path: P,
        file: &mut File,
    ) -> impl std::future::Future<Output = Result<Self, Error>> + Send;
}

async fn tell(file: &mut BufReader<&mut File>) -> Option<u64> {
    file.stream_position().await.ok()
}

async fn is_eof(file: &mut BufReader<&mut File>) -> Result<bool, Error> {
    match file.fill_buf().await {
        Ok(buffer) => Ok(buffer.is_empty()),
        Err(err) => Err(Error {
            kind: err.into(),
            offset: tell(file).await,
        }),
    }
}

async fn skip(file: &mut BufReader<&mut File>, bytes: i64) -> Result<u64, Error> {
    let offset = tell(file).await;
    file.seek(SeekFrom::Current(bytes))
        .await
        .map_err(|err| Error {
            kind: err.into(),
            offset,
        })
}

async fn deserialize_exact(
    file: &mut BufReader<&mut File>,
    len: usize,
) -> Result<(Vec<u8>, usize), Error> {
    let offset = tell(file).await;
    let mut buffer = vec![0; len];

    let result: Result<usize, ErrorKind> =
        file.read_exact(&mut buffer).await.map_err(|err| err.into());

    match result {
        Ok(bytes_read) => Ok((buffer, bytes_read)),
        Err(kind) => Err(Error { kind, offset }),
    }
}

async fn deserialize_string(
    file: &mut BufReader<&mut File>,
    len: usize,
) -> Result<(String, usize), Error> {
    let offset = tell(file).await;
    deserialize_exact(file, len)
        .await
        .and_then(|(mut buffer, bytes_read)| {
            buffer.pop_if(|last| *last == b'\0');
            String::from_utf8(buffer)
                .map(|string| (string, bytes_read))
                .map_err(|err| Error {
                    kind: err.into(),
                    offset,
                })
        })
}

async fn deserialize<'a, 'b, T, Fut: Future<Output = Result<T, tokio::io::Error>>>(
    file: &'a mut BufReader<&'b mut File>,
    mut fun: impl FnMut(&'a mut BufReader<&'b mut File>) -> Fut,
) -> Result<T, Error> {
    let offset = tell(file).await;
    match fun(file).await {
        Ok(value) => Ok(value),
        Err(err) => {
            let kind = err.into();
            Err(Error { kind, offset })
        }
    }
}

fn i32_to_usize(value: i32) -> Result<usize, Error> {
    value.try_into().map_err(|err: TryFromIntError| Error {
        kind: err.into(),
        offset: None,
    })
}

fn u32_to_usize(value: u32) -> Result<usize, Error> {
    value.try_into().map_err(|err: TryFromIntError| Error {
        kind: err.into(),
        offset: None,
    })
}

fn usize_to_i32(value: usize) -> Result<i32, Error> {
    value.try_into().map_err(|err: TryFromIntError| Error {
        kind: err.into(),
        offset: None,
    })
}

pub struct Asset {
    pub path: PathBuf,
    pub offset: u64,
}

async fn list_assets_in_file(path: PathBuf, mut file: File) -> HashMap<String, Asset> {
    let is_pack = path
        .extension()
        .map(|ext| ext.eq_ignore_ascii_case("pack"))
        .unwrap_or(false);
    match is_pack {
        true => {
            let Ok(pack) = Pack::deserialize(path.clone(), &mut file).await else {
                return HashMap::new();
            };

            pack.flatten()
        }
        false => {
            let Some(Ok(name)) = path
                .file_name()
                .map(|name| name.to_os_string().into_string())
            else {
                return HashMap::new();
            };

            let mut results: HashMap<_, _> = HashMap::new();
            results.insert(
                name,
                Asset {
                    path: path.clone(),
                    offset: 0,
                },
            );

            results
        }
    }
}

pub async fn list_assets<P: AsRef<Path>>(
    directory_path: P,
    follow_links: bool,
    mut predicate: impl FnMut(&Path) -> bool,
) -> Result<HashMap<String, Asset>, tokio::io::Error> {
    let mut futures = JoinSet::new();

    let walker = WalkDir::new(&directory_path)
        .follow_links(follow_links)
        .into_iter();
    for entry in walker.filter_map(|err| err.ok()) {
        if predicate(entry.path()) {
            // Per PathBuf#isFile():
            // When the goal is simply to read from (or write to) the source, the most reliable way
            // to test the source can be read (or written to) is to open it.
            if let Ok(file) = OpenOptions::new().read(true).open(entry.path()).await {
                futures.spawn(list_assets_in_file(entry.into_path(), file));
            }
        }
    }

    let mut final_result = HashMap::new();
    futures
        .join_all()
        .await
        .into_iter()
        .for_each(|result| final_result.extend(result.into_iter()));
    Ok(final_result)
}
