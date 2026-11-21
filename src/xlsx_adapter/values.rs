use super::*;

// Convert supported XLSX values into typed calculation results.
pub(super) fn value_as_calculation(value: &XlsxValue) -> Option<CalcValue> {
    match value {
        XlsxValue::Empty => None,
        XlsxValue::String(value) => Some(CalcValue::Text(value.clone())),
        XlsxValue::RichText(runs) => Some(CalcValue::Text(
            runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        )),
        XlsxValue::Number(value) => Some(CalcValue::Number(*value)),
        XlsxValue::Boolean(value) => Some(CalcValue::Boolean(*value)),
        XlsxValue::Error(value) => Some(CalcValue::Error(value.clone())),
    }
}
