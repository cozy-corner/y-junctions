// Test file for verifying lint-staged formatting

pub fn test_function(x: i32, y: i32) -> i32 {
    let result = x + y;
    let map = std::collections::HashMap::new();
    map.entry("key")
        .or_insert_with(|| "value".to_string());  // Should be formatted to or_default()
    result
}
