mod pack;

pub use pack::*;

use std::{collections::HashMap, path::PathBuf};

use tokio::fs::File;

pub struct Asset {
    path: PathBuf,
    offset: u64,
    size: u32,
    crc: Option<u32>,
}

pub trait DeserializeAsset: Sized {
    fn deserialize(
        path: PathBuf,
        file: &mut File,
    ) -> impl std::future::Future<Output = Result<Self, tokio::io::Error>> + Send;
}

pub trait NestedAsset {
    fn flatten(self) -> HashMap<String, Asset>;
}
