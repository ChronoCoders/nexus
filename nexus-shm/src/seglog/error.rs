use std::fmt;

use crate::error::ShmError;

#[derive(Debug)]
#[non_exhaustive]
pub enum SegmentedLogError {
    RecordTooLarge { max: usize },
    StandbyNotReady,
    Shm(ShmError),
}

impl fmt::Display for SegmentedLogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordTooLarge { max } => {
                write!(f, "payload exceeds segment capacity ({max} bytes max)")
            }
            Self::StandbyNotReady => {
                write!(f, "conductor has not finished cleaning the standby segment")
            }
            Self::Shm(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SegmentedLogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Shm(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ShmError> for SegmentedLogError {
    fn from(e: ShmError) -> Self {
        Self::Shm(e)
    }
}
