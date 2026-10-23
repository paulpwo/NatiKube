use natikube_core::{
    format_cpu, format_memory, parse_cpu_cores, parse_memory_bytes, parse_quantity,
};

#[test]
fn cpu_uses_three_decimals() {
    assert_eq!(format_cpu(0.007), "0.007");
    assert_eq!(format_cpu(0.0), "0.000");
    assert_eq!(format_cpu(1.5), "1.500");
    assert_eq!(format_cpu(0.0004), "0.000");
    assert_eq!(format_cpu(12.3456), "12.346");
}

#[test]
fn memory_uses_binary_units_with_one_decimal() {
    assert_eq!(format_memory(0), "0");
    assert_eq!(format_memory(512), "512B");
    assert_eq!(format_memory(1024), "1Ki");
    assert_eq!(format_memory(1536), "1.5Ki");
    assert_eq!(format_memory(36_280_730), "34.6Mi");
    assert_eq!(format_memory(5 * 1024 * 1024 * 1024), "5Gi");
    assert_eq!(format_memory(1_288_490_189), "1.2Gi");
}

#[test]
fn parses_plain_and_si_suffixes() {
    assert_eq!(parse_quantity("2"), Some(2.0));
    assert_eq!(parse_quantity("100m"), Some(0.1));
    assert_eq!(parse_quantity("1500u"), Some(0.0015));
    assert_eq!(parse_quantity("250000000n"), Some(0.25));
    assert_eq!(parse_quantity("1k"), Some(1000.0));
    assert_eq!(parse_quantity("5M"), Some(5_000_000.0));
    assert_eq!(parse_quantity("2G"), Some(2_000_000_000.0));
}

#[test]
fn parses_binary_suffixes_and_exponents() {
    assert_eq!(parse_quantity("1Ki"), Some(1024.0));
    assert_eq!(parse_quantity("128Mi"), Some(134_217_728.0));
    assert_eq!(parse_quantity("1Gi"), Some(1_073_741_824.0));
    assert_eq!(parse_quantity("129e6"), Some(129_000_000.0));
    assert_eq!(parse_quantity("1.5Gi"), Some(1_610_612_736.0));
}

#[test]
fn rejects_garbage() {
    assert_eq!(parse_quantity(""), None);
    assert_eq!(parse_quantity("abc"), None);
    assert_eq!(parse_quantity("12Qi"), None);
    assert_eq!(parse_quantity("1.2.3"), None);
}

#[test]
fn cpu_and_memory_helpers() {
    assert_eq!(parse_cpu_cores("7m"), Some(0.007));
    assert_eq!(parse_memory_bytes("34Mi"), Some(35_651_584));
    assert_eq!(parse_memory_bytes("nope"), None);
}
