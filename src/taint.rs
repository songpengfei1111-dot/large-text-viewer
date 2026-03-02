

pub fn test_search_count() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;
    use crate::search_service::{SearchService, SearchConfig};

    // 创建临时测试文件
    let file_path = std::path::PathBuf::from("/Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt");
    println!("Testing file: {}", file_path.display());
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader); // 会自动index

    let pattern = "string".to_string();
    let config = SearchConfig::new(pattern.clone())
        .with_regex(false)
        .with_max_results(999999999)
        .with_context(0)
        .with_line_range(None, None);

    // 只计数模式
    let count = service.count_matches(config)?;
    println!("Total matches for '{}': {}", pattern, count);

    Ok(())
}
