use sofel_type1::metrics::{Metrics, parse_metrics};

#[test]
fn adr0065_i1_type1_hsbw_and_sbw_advances() -> Result<(), sofel_type1::metrics::MetricsError> {
    assert_eq!(
        parse_metrics(&[139, 247, 142, 13, 14])?,
        Metrics {
            left: 0,
            top: 0,
            advance_x: 250,
            advance_y: 0
        }
    );
    assert_eq!(
        parse_metrics(&[139, 139, 247, 142, 139, 12, 7, 14])?,
        Metrics {
            left: 0,
            top: 0,
            advance_x: 250,
            advance_y: 0
        }
    );
    Ok(())
}

#[test]
fn adr0065_i2_type1_rejects_truncated_metric_operands() {
    assert!(parse_metrics(&[139, 247]).is_err());
    assert!(parse_metrics(&[139, 13]).is_err());
    assert!(parse_metrics(&[139, 139, 14]).is_err());
}
