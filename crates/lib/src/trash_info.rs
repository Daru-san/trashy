use std::borrow::Cow;
use std::str::FromStr;
use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};
use log::{debug, error, info, warn};
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use snafu::{OptionExt, ResultExt, Snafu};

use super::parser::{self, TRASH_DATETIME_FORMAT, parse_trash_info};
use crate::TRASH_INFO_EXT;
use crate::utils::{self, to_trash_info_dir};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display(
        "Could not convert path {:#?} to utf-8 str to do percent encoding",
        path
    ))]
    Utf8PercentEncode {
        path: PathBuf,
    },

    #[snafu(display(
        "Percent-decoded bytes of {} are not well-formed in UTF-8: {}",
        string,
        source
    ))]
    Utf8PercentDecode {
        string: String,
        source: core::str::Utf8Error,
    },

    #[snafu(display("Failed to open file with path {}: {}", path.display(), source))]
    FileOpen {
        source: io::Error,
        path: PathBuf,
    },

    #[snafu(display("Failed to write to trash info file: {}", source))]
    TrashInfoWrite {
        source: io::Error,
    },

    #[snafu(display("Failed to convert path {} to string {}", path.display(), source))]
    ConvertToStr {
        source: utils::Error,
        path: PathBuf,
    },

    #[snafu(display("Failed to move directory from {} to {}: {}", from.display(), to.display(), source))]
    MoveDir {
        source: fs_extra::error::Error,
        from: PathBuf,
        to: PathBuf,
    },

    #[snafu(display("Failed to move file from {} to {}: {}", from.display(), to.display(), source))]
    MoveFile {
        source: fs_extra::error::Error,
        from: PathBuf,
        to: PathBuf,
    },

    ReadToStr {
        path: PathBuf,
    },

    #[snafu(context(false))]
    ParseTrashInfo {
        source: parser::Error,
    },

    WrongExtension {
        path: PathBuf,
    },

    NoExtension {
        path: PathBuf,
    },
}

type Result<T, E = Error> = ::std::result::Result<T, E>;

#[derive(Debug, Eq, PartialEq)]
pub struct TrashInfo {
    percent_path: String,
    deletion_date: NaiveDateTime,
}

impl TrashInfo {
    pub(super) fn new(
        real_path: impl AsRef<Path>,
        deletion_date: Option<NaiveDateTime>,
    ) -> Result<Self> {
        let path = real_path.as_ref();
        let path = path.to_str().context(Utf8PercentEncode { path })?;
        let path = utf8_percent_encode(path, NON_ALPHANUMERIC).to_string();
        let deletion_date = deletion_date.unwrap_or(Local::now().naive_local());

        Ok(TrashInfo {
            percent_path: path,
            deletion_date,
        })
    }

    /// saves the name with the extension .trashinfo
    pub(super) fn save(self, name: &str) -> Result<()> {
        let mut name = PathBuf::from(name);
        name.set_extension(TRASH_INFO_EXT);
        let path = to_trash_info_dir(name);

        let mut trash_info_file = OpenOptions::new()
            .read(false)
            .write(true)
            .create(false)
            .create_new(true)
            .append(false)
            .truncate(false)
            .open(&path)
            .context(FileOpen { path })?;

        trash_info_file
            .write_all(self.to_string().as_bytes())
            .context(TrashInfoWrite)?;

        Ok(())
    }

    pub(crate) fn parse_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        check_extension(path)?;
        let trash_info = fs::read_to_string(path)
            .context(ReadToStr { path })?
            .parse::<TrashInfo>()?;
        Ok(trash_info)
    }

    /// Returns the path as a percent encoded string
    pub fn path(&self) -> &str {
        &self.percent_path
    }

    /// Returns the path as a percent decoded string
    pub fn path_decoded(&self) -> Result<Cow<'_, str>> {
        let decoded_str = percent_decode_str(&self.percent_path)
            .decode_utf8()
            .context(Utf8PercentDecode {
                string: &self.percent_path,
            })?;

        Ok(decoded_str)
    }

    /// Gets the deletion date
    pub fn deletion_date(&self) -> NaiveDateTime {
        self.deletion_date
    }

    /// Gets the deletions date as a string formated using the trash_info_format
    pub fn deletion_date_string_format(&self) -> String {
        format!("{}", self.deletion_date.format(TRASH_DATETIME_FORMAT))
    }
}

/// Checks if the extension is correct or no extension
fn check_extension(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if let Some(ext) = path.extension() {
        if ext != TRASH_INFO_EXT {
            WrongExtension { path }.fail();
        }
    } else {
        NoExtension { path }.fail();
    }
    Ok(())
}

impl FromStr for TrashInfo {
    type Err = Error;

    fn from_str(s: &str) -> Result<TrashInfo> {
        let trash_info = parse_trash_info(s)?;
        Ok(trash_info)
    }
}

impl fmt::Display for TrashInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[Trash Info]\nPath={}\nDeletionDate={}",
            self.percent_path,
            self.deletion_date_string_format(),
        )
    }
}

impl Ord for TrashInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deletion_date.cmp(&other.deletion_date)
    }
}

impl PartialOrd for TrashInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}