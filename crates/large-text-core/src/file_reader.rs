use anyhow::Result;
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1252};
use memmap2::Mmap;
use rayon::prelude::*;
use std::fs::File;
use std::path::PathBuf;

pub struct FileReader {
    mmap: Mmap,
    path: PathBuf,
    encoding: &'static Encoding,
}

impl FileReader {
    pub fn new(path: PathBuf, encoding: &'static Encoding) -> Result<Self> {
        let file = File::open(&path)?; //?用于简化错误处理，自动将错误类型转换为函数的返回错误类型
        let metadata = file.metadata()?;
        if metadata.len() == 0 {
            anyhow::bail!("Cannot memory-map an empty file: {:?}", path); // anyhow 库提供的宏，用于快速返回错误
        }

        //unsafe 块，因为内存映射操作涉及原始指针和系统调用
        let mmap = unsafe {
            let mmap = Mmap::map(&file)?; //将整个文件映射到进程的虚拟内存空间
            #[cfg(unix)] //条件编译属性：这段代码只在 Unix 系统（Linux、macOS 等）上编译
            {
                libc::madvise(
                    mmap.as_ptr() as *mut libc::c_void,
                    mmap.len(),
                    libc::MADV_SEQUENTIAL | libc::MADV_WILLNEED, //预读优化，提前加载
                );
            }
            mmap
        };

        Ok(Self {mmap, path, encoding,})
    }

    pub fn get_chunk(&self, start: usize, end: usize) -> String {
        // 用于从内存映射文件中提取指定范围的文本并解码为 Rust 字符串：
        let end = end.min(self.mmap.len());
        if start >= end {
            return String::new();
        }

        let bytes = &self.mmap[start..end];
        let (cow, _encoding, _had_errors) = self.encoding.decode(bytes);
        cow.into_owned()
    }

    pub fn get_bytes(&self, start: usize, end: usize) -> &[u8] {
        let end = end.min(self.mmap.len());
        if start >= end {
            return &[];
        }
        &self.mmap[start..end]
    }

    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    pub fn all_data(&self) -> &[u8] {
        &self.mmap[..]
    }

    /// 最快版本：32字节展开 + 并行处理
    pub fn find_line_offsets(&self) -> Vec<usize> {
        // 映射行号
        let data = self.all_data();
        let len = data.len();

        if len == 0 {
            return vec![0];
        }

        let num_threads = rayon::current_num_threads();
        let chunk_size = len / num_threads;

        let chunk_results: Vec<Vec<usize>> = (0..num_threads)
            .into_par_iter()
            .map(|thread_id| {
                let chunk_start = thread_id * chunk_size;
                let chunk_end = if thread_id == num_threads - 1 {
                    len
                } else {
                    (thread_id + 1) * chunk_size
                };

                let chunk = &data[chunk_start..chunk_end];
                let chunk_len = chunk.len();
                let mut local_offsets = Vec::with_capacity(chunk_len / 65);

                if thread_id == 0 {
                    local_offsets.push(0);
                }

                let mut i = 0;

                // 32字节对齐处理 - 完全展开
                while i + 32 <= chunk_len {
                    unsafe {
                        let ptr = chunk.as_ptr().add(i);
                        let bytes = std::ptr::read_unaligned(ptr as *const [u8; 32]);

                        if bytes[0] == b'\n' { local_offsets.push(chunk_start + i + 1); }
                        if bytes[1] == b'\n' { local_offsets.push(chunk_start + i + 2); }
                        if bytes[2] == b'\n' { local_offsets.push(chunk_start + i + 3); }
                        if bytes[3] == b'\n' { local_offsets.push(chunk_start + i + 4); }
                        if bytes[4] == b'\n' { local_offsets.push(chunk_start + i + 5); }
                        if bytes[5] == b'\n' { local_offsets.push(chunk_start + i + 6); }
                        if bytes[6] == b'\n' { local_offsets.push(chunk_start + i + 7); }
                        if bytes[7] == b'\n' { local_offsets.push(chunk_start + i + 8); }
                        if bytes[8] == b'\n' { local_offsets.push(chunk_start + i + 9); }
                        if bytes[9] == b'\n' { local_offsets.push(chunk_start + i + 10); }
                        if bytes[10] == b'\n' { local_offsets.push(chunk_start + i + 11); }
                        if bytes[11] == b'\n' { local_offsets.push(chunk_start + i + 12); }
                        if bytes[12] == b'\n' { local_offsets.push(chunk_start + i + 13); }
                        if bytes[13] == b'\n' { local_offsets.push(chunk_start + i + 14); }
                        if bytes[14] == b'\n' { local_offsets.push(chunk_start + i + 15); }
                        if bytes[15] == b'\n' { local_offsets.push(chunk_start + i + 16); }
                        if bytes[16] == b'\n' { local_offsets.push(chunk_start + i + 17); }
                        if bytes[17] == b'\n' { local_offsets.push(chunk_start + i + 18); }
                        if bytes[18] == b'\n' { local_offsets.push(chunk_start + i + 19); }
                        if bytes[19] == b'\n' { local_offsets.push(chunk_start + i + 20); }
                        if bytes[20] == b'\n' { local_offsets.push(chunk_start + i + 21); }
                        if bytes[21] == b'\n' { local_offsets.push(chunk_start + i + 22); }
                        if bytes[22] == b'\n' { local_offsets.push(chunk_start + i + 23); }
                        if bytes[23] == b'\n' { local_offsets.push(chunk_start + i + 24); }
                        if bytes[24] == b'\n' { local_offsets.push(chunk_start + i + 25); }
                        if bytes[25] == b'\n' { local_offsets.push(chunk_start + i + 26); }
                        if bytes[26] == b'\n' { local_offsets.push(chunk_start + i + 27); }
                        if bytes[27] == b'\n' { local_offsets.push(chunk_start + i + 28); }
                        if bytes[28] == b'\n' { local_offsets.push(chunk_start + i + 29); }
                        if bytes[29] == b'\n' { local_offsets.push(chunk_start + i + 30); }
                        if bytes[30] == b'\n' { local_offsets.push(chunk_start + i + 31); }
                        if bytes[31] == b'\n' { local_offsets.push(chunk_start + i + 32); }
                    }
                    i += 32;
                }

                // 处理剩余字节
                while i < chunk_len {
                    if chunk[i] == b'\n' {
                        local_offsets.push(chunk_start + i + 1);
                    }
                    i += 1;
                }

                local_offsets
            })
            .collect();

        let total_size: usize = chunk_results.iter().map(|v| v.len()).sum();
        let mut line_offsets = Vec::with_capacity(total_size);

        for chunk_result in chunk_results {
            line_offsets.extend(chunk_result);
        }

        line_offsets
    }
}

pub fn detect_encoding(bytes: &[u8]) -> &'static Encoding {
    // Check for BOM
    if bytes.len() >= 3 && bytes[0..3] == [0xEF, 0xBB, 0xBF] {
        return UTF_8;
    }
    if bytes.len() >= 2 {
        if bytes[0..2] == [0xFF, 0xFE] {
            return UTF_16LE;
        }
        if bytes[0..2] == [0xFE, 0xFF] {
            return UTF_16BE;
        }
    }

    // Try UTF-8 validation
    if std::str::from_utf8(bytes).is_ok() {
        return UTF_8;
    }

    // Default to WINDOWS_1252 (similar to ISO-8859-1)
    WINDOWS_1252
}

pub fn available_encodings() -> Vec<(&'static str, &'static Encoding)> {
    vec![
        ("UTF-8", UTF_8),
        ("UTF-16 LE", UTF_16LE),
        ("UTF-16 BE", UTF_16BE),
        ("Windows-1252", WINDOWS_1252),
        ("ISO-8859-1", encoding_rs::WINDOWS_1252), // Similar enough
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_detect_encoding() {
        assert_eq!(detect_encoding(b"\xEF\xBB\xBFhello"), UTF_8);
        assert_eq!(detect_encoding(b"\xFF\xFEhello"), UTF_16LE);
        assert_eq!(detect_encoding(b"\xFE\xFFhello"), UTF_16BE);
        assert_eq!(detect_encoding(b"hello world"), UTF_8);
        // Invalid UTF-8 sequence
        assert_eq!(detect_encoding(b"\xFF\xFF\xFF"), WINDOWS_1252);
    }

    #[test]
    fn test_file_reader() -> Result<()> {
        let mut file = NamedTempFile::new()?;
        write!(file, "Hello World\nLine 2")?;
        let path = file.path().to_path_buf();

        let reader = FileReader::new(path.clone(), UTF_8)?;
        assert_eq!(reader.len(), 18);
        assert_eq!(reader.get_chunk(0, 5), "Hello");
        assert_eq!(reader.get_chunk(6, 11), "World");
        assert_eq!(reader.get_bytes(0, 5), b"Hello");

        Ok(())
    }

    #[test]
    fn test_empty_file() -> Result<()> {
        let file = NamedTempFile::new()?;
        let path = file.path().to_path_buf();
        let result = FileReader::new(path, UTF_8);
        assert!(result.is_err());
        Ok(())
    }
}
