use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path, path::PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Dimensions {
    pub width: f64,
    pub height: f64,
}

impl Dimensions {
    fn is_valid(self) -> bool {
        [self.width, self.height]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
    }

    fn differs_from(self, other: Self) -> bool {
        (self.width - other.width).abs() > 1.0 || (self.height - other.height).abs() > 1.0
    }
}

#[derive(Default)]
pub struct PickerSize {
    pub preferred: Option<Dimensions>,
    file: Option<PathBuf>,
}

impl PickerSize {
    pub fn load(file: Option<PathBuf>) -> Self {
        let preferred = file
            .as_ref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Dimensions>(&bytes).ok())
            .filter(|size| size.is_valid());
        Self { preferred, file }
    }

    pub fn remember(
        &mut self,
        presented: Option<Dimensions>,
        actual: Dimensions,
    ) -> io::Result<()> {
        let Some(presented) = presented else {
            return Ok(());
        };
        if !actual.is_valid() || !actual.differs_from(presented) {
            return Ok(());
        }
        self.preferred = Some(actual);
        if let Some(path) = &self.file {
            let directory = path.parent().unwrap_or_else(|| Path::new("."));
            fs::create_dir_all(directory)?;
            let mut staged = tempfile::NamedTempFile::new_in(directory)?;
            serde_json::to_writer(staged.as_file_mut(), &actual)?;
            staged.persist(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_manual_sizes_across_restarts_but_ignores_automatic_fitting() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("picker-size.json");
        let mut state = PickerSize::load(Some(file.clone()));
        let automatic = Dimensions {
            width: 460.0,
            height: 615.0,
        };
        state.remember(Some(automatic), automatic).unwrap();
        assert!(!file.exists());

        let manual = Dimensions {
            width: 480.0,
            height: 550.0,
        };
        state.remember(Some(automatic), manual).unwrap();
        assert_eq!(PickerSize::load(Some(file.clone())).preferred, Some(manual));

        let clamped = Dimensions {
            width: 480.0,
            height: 450.0,
        };
        state.remember(Some(clamped), clamped).unwrap();
        assert_eq!(PickerSize::load(Some(file)).preferred, Some(manual));
    }
}
