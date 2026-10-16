use std::io::{self, Seek, Write};
use std::path::Path;

use async_trait::async_trait;
use tokio::io::AsyncRead;

pub struct ExportFileSource {
    pub length: u64,
    pub reader: Box<dyn AsyncRead + Unpin + Send>,
}

/// 只打开历史记录已解析出的本地普通文件，不接受调用方提供的任意路径。
#[async_trait]
pub trait ExportFileSourcePort: Send + Sync {
    async fn open(&self, path: &Path) -> io::Result<ExportFileSource>;
}

/// 用户选择的输出目标；定位写入供 ZIP 回填头部，完成仅在整个归档成功后调用。
pub trait FileExportDestination: Write + Seek + Send {
    fn ensure_open(&self) -> io::Result<()>;
    fn complete(&mut self) -> io::Result<()>;
}
