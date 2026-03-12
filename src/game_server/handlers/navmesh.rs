use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsStr,
    io::SeekFrom,
    path::Path,
};

use asset_serialize::{list_assets, Asset, DeserializeAsset};
use tokio::{
    fs::OpenOptions,
    io::{AsyncSeekExt, BufReader},
    task::JoinSet,
};

use crate::ConfigError;

#[derive(Debug)]
pub enum AssetCacheError {
    Deserialize(asset_serialize::Error),
    Io(tokio::io::Error),
    NotFound,
}

impl From<asset_serialize::Error> for AssetCacheError {
    fn from(value: asset_serialize::Error) -> Self {
        AssetCacheError::Deserialize(value)
    }
}

impl From<tokio::io::Error> for AssetCacheError {
    fn from(value: tokio::io::Error) -> Self {
        AssetCacheError::Io(value)
    }
}

pub struct AssetCache {
    assets: BTreeMap<String, Asset>,
}

impl AssetCache {
    pub async fn new<P: AsRef<Path>>(path: P, extensions: &[&str]) -> Result<Self, ConfigError> {
        let extensions: HashSet<&str> = HashSet::from_iter(extensions.iter().copied());
        let assets = list_assets(path, false, |path| {
            path.extension()
                .and_then(OsStr::to_str)
                .map(|extension| extensions.contains(&extension))
                .unwrap_or_default()
        })
        .await?;

        Ok(AssetCache { assets })
    }

    pub fn filter(&self, prefix: &str, mut predicate: impl FnMut(&str) -> bool) -> Vec<&str> {
        self.assets
            .range(prefix.to_string()..)
            .map(|(name, _)| &name[..])
            .take_while(|name| name.starts_with(prefix))
            .filter(|name| predicate(name))
            .collect()
    }

    pub async fn deserialize<T: DeserializeAsset + Send + 'static>(
        &self,
        asset_names: Vec<&str>,
    ) -> (Vec<(String, T)>, Vec<(String, AssetCacheError)>) {
        let mut futures = JoinSet::new();
        let mut errors = Vec::new();

        for asset_name in asset_names {
            let asset_name = asset_name.to_string();
            let asset = match self.assets.get(&asset_name) {
                Some(asset) => asset.clone(),
                None => {
                    errors.push((asset_name, AssetCacheError::NotFound));
                    continue;
                }
            };

            let task = async move || {
                let mut file = OpenOptions::new().read(true).open(&asset.path).await?;
                file.seek(SeekFrom::Start(asset.offset)).await?;
                let mut reader = BufReader::new(file);
                let deserialized_asset = T::deserialize(asset.path, &mut reader).await?;
                Ok(deserialized_asset)
            };

            futures.spawn(async {
                match task().await {
                    Ok(deserialized_asset) => Ok((asset_name, deserialized_asset)),
                    Err(err) => Err((asset_name, err)),
                }
            });
        }

        let results = futures.join_all().await;

        let mut deserialized_assets = Vec::new();
        for result in results.into_iter() {
            match result {
                Ok(deserialiazed_asset) => deserialized_assets.push(deserialiazed_asset),
                Err(err) => errors.push(err.into()),
            }
        }

        (deserialized_assets, errors)
    }
}
