use linkedin::browser::stealth::STEALTH_JS_PAYLOAD;
use linkedin::infra::typing::{get_adjacent_qwerty_key, sample_gaussian_delay, HumanTypingConfig};

#[test]
fn test_gaussian_delay_sampling_within_bounds() {
    let min = 50;
    let max = 150;
    let mut sum = 0u64;
    let iterations = 1000;

    for _ in 0..iterations {
        let delay = sample_gaussian_delay(min, max);
        assert!(delay >= min, "Delay {delay} should be >= min {min}");
        assert!(delay <= max, "Delay {delay} should be <= max {max}");
        sum += delay;
    }

    let avg = sum as f64 / iterations as f64;
    let expected_mean = (min + max) as f64 / 2.0; // 100.0

    // Average across 1000 iterations should be close to the mean (within +/- 8ms)
    assert!(
        (avg - expected_mean).abs() < 8.0,
        "Sampled average {avg} should be close to expected mean {expected_mean}"
    );
}

#[test]
fn test_gaussian_delay_when_min_equals_max() {
    let delay = sample_gaussian_delay(100, 100);
    assert_eq!(delay, 100);

    let delay_inverted = sample_gaussian_delay(200, 100);
    assert_eq!(delay_inverted, 200);
}

#[test]
fn test_adjacent_qwerty_keys() {
    assert_eq!(get_adjacent_qwerty_key('a'), 's');
    assert_eq!(get_adjacent_qwerty_key('A'), 'S');
    assert_eq!(get_adjacent_qwerty_key('q'), 'w');
    assert_eq!(get_adjacent_qwerty_key('Q'), 'W');
    assert_eq!(get_adjacent_qwerty_key('p'), 'o');
    assert_eq!(get_adjacent_qwerty_key('l'), 'k');
}

#[test]
fn test_human_typing_config_defaults() {
    let cfg = HumanTypingConfig::default();
    assert_eq!(cfg.min_delay_ms, 45);
    assert_eq!(cfg.max_delay_ms, 180);
    assert!((cfg.typo_rate - 0.015).abs() < f64::EPSILON);
}

#[test]
fn test_stealth_js_payload_integrity() {
    assert!(STEALTH_JS_PAYLOAD.contains("webdriver"));
    assert!(STEALTH_JS_PAYLOAD.contains("window.chrome"));
    assert!(STEALTH_JS_PAYLOAD.contains("Intel Inc."));
    assert!(STEALTH_JS_PAYLOAD.contains("Intel Iris OpenGL Engine"));
    assert!(STEALTH_JS_PAYLOAD.contains("PDF Viewer"));
    assert!(STEALTH_JS_PAYLOAD.contains("en-US"));
}
