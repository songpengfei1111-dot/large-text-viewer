use crate::file_reader::FileReader;

pub struct LineIndexer {
    line_offsets: Vec<usize>,
    total_lines: usize,
}

impl Default for LineIndexer {
    fn default() -> Self {
        Self::new()
    }
}

impl LineIndexer {
    pub fn new() -> Self {
        Self {
            line_offsets: vec![0],
            total_lines: 0,
        }
    }

    pub fn index_file(&mut self, reader: &FileReader) {
        // 直接一次性获取所有行号偏移
        // 这个过程可以是异步的
        self.line_offsets = reader.find_line_offsets();
        self.total_lines = self.line_offsets.len();
    }

    pub fn get_line_range(&self, line_num: usize) -> Option<(usize, usize)> {
        if line_num >= self.line_offsets.len() {
            return None;
        }

        let start = self.line_offsets[line_num];
        let end = if line_num + 1 < self.line_offsets.len() {
            self.line_offsets[line_num + 1]
        } else {
            usize::MAX
        };

        Some((start, end))
    }

    pub fn find_line_at_offset(&self, offset: usize) -> usize {
        match self.line_offsets.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        }
    }

    pub fn total_lines(&self) -> usize {
        self.total_lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_reader::detect_encoding;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_line_indexer_small_file() -> anyhow::Result<()> {
        let mut file = NamedTempFile::new()?;
        write!(file, "Line 1\nLine 2\nLine 3")?;
        let path = file.path().to_path_buf();

        let reader = FileReader::new(path, detect_encoding(b""))?;
        let mut indexer = LineIndexer::new();
        indexer.index_file(&reader);

        assert_eq!(indexer.total_lines, 3);
        assert_eq!(indexer.line_offsets, vec![0, 7, 14]);
        Ok(())
    }

    #[test]
    fn test_line_indexer_empty_lines() -> anyhow::Result<()> {
        let mut file = NamedTempFile::new()?;
        write!(file, "\n\n\n")?;
        let path = file.path().to_path_buf();

        let reader = FileReader::new(path, detect_encoding(b""))?;
        let mut indexer = LineIndexer::new();
        indexer.index_file(&reader);

        assert_eq!(indexer.total_lines, 4);
        assert_eq!(indexer.line_offsets, vec![0, 1, 2, 3]);
        Ok(())
    }
}
