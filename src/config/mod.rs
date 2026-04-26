use std::{
    fs::{self, File},
    io::Error,
    path::{Path, PathBuf},
};

use serde_yaml::{Mapping, Value};

#[allow(dead_code)]
#[derive(Debug)]
pub enum ConfigError {
    Io(Error),
    Deserialize(serde_yaml::Error),
    ConstraintViolated(String),
    BvhDeserialize(pot::Error),
}

impl From<Error> for ConfigError {
    fn from(value: Error) -> Self {
        ConfigError::Io(value)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(value: serde_yaml::Error) -> Self {
        ConfigError::Deserialize(value)
    }
}

impl From<pot::Error> for ConfigError {
    fn from(value: pot::Error) -> Self {
        ConfigError::BvhDeserialize(value)
    }
}

fn list_config_yamls(root: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    let mut files_with_yaml = Vec::new();

    for entry in fs::read_dir(root)? {
        let entry_path = entry?.path();
        match entry_path.is_dir() {
            true => files_with_yaml.append(&mut list_config_yamls(&entry_path)?),
            false => {
                if entry_path
                    .extension()
                    .is_some_and(|extension| extension == "yaml")
                {
                    files_with_yaml.push(entry_path);
                }
            }
        }
    }

    Ok(files_with_yaml)
}

fn merge_yaml_values(
    yaml_path: &str,
    mut accumulated_map: Mapping,
    incoming_map: Mapping,
) -> Result<Mapping, ConfigError> {
    for (key, incoming_value) in incoming_map {
        let Some(existing_value) = accumulated_map.remove(&key) else {
            accumulated_map.insert(key, incoming_value);
            continue;
        };

        let child_yaml_path = format!("{yaml_path} > {:?}", key);
        match (existing_value, incoming_value) {
            (Value::Mapping(existing_map), Value::Mapping(incoming_map)) => {
                let merged = merge_yaml_values(&child_yaml_path, existing_map, incoming_map)?;
                accumulated_map.insert(key, Value::Mapping(merged));
            }
            _ => {
                return Err(ConfigError::ConstraintViolated(format!(
                    "Unable to merge {child_yaml_path}: both values for this key must be maps"
                )));
            }
        }
    }

    Ok(accumulated_map)
}

pub fn merge_config_dir(root: &Path) -> Result<Mapping, ConfigError> {
    let mut accumulated_map = Mapping::new();
    for path in list_config_yamls(root)? {
        let file = File::open(&path)?;
        accumulated_map = merge_yaml_values("", accumulated_map, serde_yaml::from_reader(file)?)?;
    }

    Ok(accumulated_map)
}
