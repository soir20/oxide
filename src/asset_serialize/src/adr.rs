use std::path::PathBuf;

use tokio::fs::File;

use crate::{DeserializeAsset, Error};

pub enum SkeletonType {
    FileName = 1,
}

/*impl DeserializeAsset for SkeletonType {
    fn deserialize(
        path: PathBuf,
        file: &mut File,
    ) -> impl std::future::Future<Output = Result<Self, Error>> + Send {
        todo!()
    }
}*/

pub struct SkeletonEntry {
    pub skeleton_type: SkeletonType,
    pub compressed_len: u16,
    pub decompressed_len: u32,
}

pub struct Adr {}
