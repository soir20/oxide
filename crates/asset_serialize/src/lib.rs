pub mod adr;
pub mod bvh;
pub mod cdt;
pub mod gcnk;
pub mod pack;

use walkdir::WalkDir;

use std::{
    any::type_name,
    collections::HashMap,
    future::Future,
    io::SeekFrom,
    path::{Path, PathBuf},
    string::FromUtf8Error,
};

use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader},
    task::JoinSet,
};

use crate::pack::Pack;

#[derive(Debug)]
pub enum ErrorKind {
    IntegerOverflow {
        expected_bytes: usize,
        actual_bytes: usize,
    },
    InvalidUtf8(FromUtf8Error),
    Io(tokio::io::Error),
    NegativeLen(i32),
    TryFromInt {
        value: String,
        from_type: &'static str,
        to_type: &'static str,
    },
    UnexpectedDecompressedLen {
        expected_decompressed_len: usize,
        actual_decompressed_len: usize,
    },
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

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: Option<u64>,
}

pub trait DeserializeAsset: Sized {
    fn deserialize<R: AsyncReader, P: AsRef<Path> + Send>(
        path: P,
        file: &mut R,
    ) -> impl std::future::Future<Output = Result<Self, Error>> + Send;
}

pub trait SerializeAsset: Sized {
    fn serialize<W: AsyncWriter + Send>(
        &self,
        file: &mut W,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}

pub trait AsyncReader: AsyncSeekExt + AsyncBufReadExt + Unpin + Send {}
impl<R: AsyncSeekExt + AsyncBufReadExt + Unpin + Send> AsyncReader for R {}
pub trait AsyncWriter: AsyncSeekExt + AsyncWriteExt + Unpin {}
impl<W: AsyncSeekExt + AsyncWriteExt + Unpin> AsyncWriter for W {}

async fn tell<R: AsyncSeekExt + Unpin>(file: &mut R) -> Option<u64> {
    file.stream_position().await.ok()
}

async fn is_eof<R: AsyncSeekExt + AsyncBufReadExt + Unpin>(file: &mut R) -> Result<bool, Error> {
    match file.fill_buf().await {
        Ok(buffer) => Ok(buffer.is_empty()),
        Err(err) => Err(Error {
            kind: err.into(),
            offset: tell(file).await,
        }),
    }
}

async fn skip<R: AsyncSeekExt + Unpin>(file: &mut R, bytes: i64) -> Result<u64, Error> {
    let offset = tell(file).await;
    file.seek(SeekFrom::Current(bytes))
        .await
        .map_err(|err| Error {
            kind: err.into(),
            offset,
        })
}

async fn deserialize_exact<R: AsyncReader>(
    file: &mut R,
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

async fn serialize_exact<W: AsyncWriter>(file: &mut W, data: &[u8]) -> Result<usize, Error> {
    serialize(file, W::write_all, data).await?;
    Ok(data.len())
}

async fn deserialize_string<R: AsyncReader>(
    file: &mut R,
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

async fn deserialize_null_terminated_string<R: AsyncReader>(
    file: &mut R,
) -> Result<(String, usize), Error> {
    let offset = tell(file).await;
    deserialize(file, async |file| {
        let mut buffer = Vec::new();
        let bytes_read = file.read_until(b'\0', &mut buffer).await?;
        Ok((buffer, bytes_read))
    })
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

async fn deserialize_f32_le_vec3<R: AsyncReader>(file: &mut R) -> Result<[f32; 3], Error> {
    Ok([
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
    ])
}

async fn deserialize_f32_le_vec4<R: AsyncReader>(file: &mut R) -> Result<[f32; 4], Error> {
    Ok([
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
        deserialize(file, R::read_f32_le).await?,
    ])
}

async fn serialize_string<W: AsyncWriter>(file: &mut W, str: &str) -> Result<usize, Error> {
    let mut bytes_written = serialize_exact(file, str.as_bytes()).await?;

    if !str.ends_with('\0') {
        serialize(file, W::write_u8, 0).await?;
        match bytes_written.checked_add(1) {
            Some(new_bytes_written) => bytes_written = new_bytes_written,
            None => {
                let expected_bytes = (usize::BITS / 8) as usize;
                return Err(Error {
                    kind: ErrorKind::IntegerOverflow {
                        expected_bytes,
                        actual_bytes: expected_bytes + 1,
                    },
                    offset: tell(file).await,
                });
            }
        }
    }

    Ok(bytes_written)
}

async fn deserialize<
    'a,
    R: AsyncSeekExt + AsyncReadExt + Unpin + 'a,
    T,
    Fut: Future<Output = Result<T, tokio::io::Error>>,
>(
    file: &'a mut R,
    mut fun: impl FnMut(&'a mut R) -> Fut,
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

async fn serialize<
    'a,
    W: AsyncSeekExt + Unpin + 'a,
    T,
    Fut: Future<Output = Result<(), tokio::io::Error>>,
>(
    file: &'a mut W,
    mut fun: impl FnMut(&'a mut W, T) -> Fut,
    value: T,
) -> Result<(), Error> {
    let offset = tell(file).await;
    match fun(file, value).await {
        Ok(value) => Ok(value),
        Err(err) => {
            let kind = err.into();
            Err(Error { kind, offset })
        }
    }
}

fn i32_to_usize(value: i32) -> Result<usize, Error> {
    value.try_into().map_err(|_| Error {
        kind: ErrorKind::TryFromInt {
            value: format!("{}", value),
            from_type: type_name::<i32>(),
            to_type: type_name::<usize>(),
        },
        offset: None,
    })
}

fn i32_to_u64(value: i32) -> Result<u64, Error> {
    value.try_into().map_err(|_| Error {
        kind: ErrorKind::TryFromInt {
            value: format!("{}", value),
            from_type: type_name::<i32>(),
            to_type: type_name::<u64>(),
        },
        offset: None,
    })
}

fn u32_to_usize(value: u32) -> Result<usize, Error> {
    value.try_into().map_err(|_| Error {
        kind: ErrorKind::TryFromInt {
            value: format!("{}", value),
            from_type: type_name::<u32>(),
            to_type: type_name::<usize>(),
        },
        offset: None,
    })
}

fn usize_to_i32(value: usize) -> Result<i32, Error> {
    value.try_into().map_err(|_| Error {
        kind: ErrorKind::TryFromInt {
            value: format!("{}", value),
            from_type: type_name::<usize>(),
            to_type: type_name::<i32>(),
        },
        offset: None,
    })
}

pub struct Asset {
    pub path: PathBuf,
    pub offset: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetType {
    Adr,
    Cdt,
    Gcnk,
    Pack,
    Unknown,
}

impl<P: AsRef<Path>> From<P> for AssetType {
    fn from(path: P) -> Self {
        let Some(extension) = path.as_ref().extension() else {
            return AssetType::Unknown;
        };
        match extension.to_ascii_lowercase().to_str() {
            Some("adr") => AssetType::Adr,
            Some("cdt") => AssetType::Cdt,
            Some("gcnk") => AssetType::Gcnk,
            Some("pack") => AssetType::Pack,
            _ => AssetType::Unknown,
        }
    }
}

async fn list_assets_in_file<P: AsRef<Path> + Clone + Send>(
    path: P,
    mut file: File,
) -> HashMap<String, Asset> {
    let is_pack = AssetType::from(&path) == AssetType::Pack;
    match is_pack {
        true => {
            let mut reader = BufReader::new(&mut file);
            let Ok(pack) = <Pack as DeserializeAsset>::deserialize(path.clone(), &mut reader).await
            else {
                return HashMap::new();
            };

            pack.flatten()
        }
        false => {
            let Some(Ok(name)) = path
                .as_ref()
                .file_name()
                .map(|name| name.to_os_string().into_string())
            else {
                return HashMap::new();
            };

            let mut results: HashMap<_, _> = HashMap::new();
            results.insert(
                name,
                Asset {
                    path: path.as_ref().to_path_buf(),
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
