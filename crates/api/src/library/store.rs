use super::blobs::Blobs;
use crate::store::StoreError;
use sqlx::PgPool;
use std::io;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("incomplete upload")]
    Incomplete,
    #[error("upload exceeds {0} bytes")]
    TooLarge(u64),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Store(#[from] StoreError),
}
impl From<sqlx::Error> for LibraryError {
    fn from(e: sqlx::Error) -> Self {
        Self::Store(e.into())
    }
}
impl From<LibraryError> for crate::error::ApiError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::Incomplete => Self::BadRequest("incomplete upload".into()),
            LibraryError::TooLarge(max) => {
                Self::PayloadTooLarge(crate::printers::files::too_large_message(max))
            }
            _ => Self::Internal("internal error".into()),
        }
    }
}

pub struct LibraryStore {
    blobs: Arc<Blobs>,
    pool: PgPool,
}
impl LibraryStore {
    pub fn open(dir: &Path, pool: PgPool) -> Result<Self, StoreError> {
        Ok(Self {
            blobs: Arc::new(Blobs::open(dir)?),
            pool,
        })
    }
    pub fn temporary(pool: PgPool) -> Result<Self, StoreError> {
        Ok(Self {
            blobs: Arc::new(Blobs::temporary()?),
            pool,
        })
    }
    pub fn blobs(&self) -> Arc<Blobs> {
        self.blobs.clone()
    }
    pub async fn reconcile(&self) -> Result<(), StoreError> {
        self.blobs.reconcile(&self.pool).await
    }
}
