use serde_json::Value;

fn value<'a>(object: &'a Value, key: &str) -> Option<&'a Value> {
    object.as_object().and_then(|map| {
        map.get(key)
            .or_else(|| map.get(&key.to_ascii_lowercase()))
            .or_else(|| map.get(snake_case(key).as_str()))
    })
}

fn snake_case(key: &str) -> String {
    let mut result = String::with_capacity(key.len() + 4);
    for (index, character) in key.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PtsStatistics {
    pub first_pts: Option<i64>,
    pub last_pts: Option<i64>,
    pub minimum_step: i64,
    pub maximum_step: i64,
    pub negative_steps: i32,
    pub zero_steps: i32,
    pub median_step: f64,
    pub abnormal_steps: i32,
    pub abnormal_ratio: f64,
    pub steps: Vec<i64>,
    pub issue_codes: Vec<String>,
}

pub fn pts_statistics(values: &[i64]) -> PtsStatistics {
    let first_pts = values.first().copied();
    let last_pts = values.last().copied();
    if values.len() < 3 {
        return PtsStatistics {
            first_pts,
            last_pts,
            minimum_step: 0,
            maximum_step: 0,
            negative_steps: 0,
            zero_steps: 0,
            median_step: 0.0,
            abnormal_steps: 0,
            abnormal_ratio: 0.0,
            steps: Vec::new(),
            issue_codes: vec!["PTS_TOO_FEW".to_string()],
        };
    }

    let steps: Vec<i64> = values.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let mut ordered = steps.clone();
    ordered.sort_unstable();
    let middle = ordered.len() / 2;
    let median_step = if ordered.len() % 2 == 1 {
        ordered[middle] as f64
    } else {
        (ordered[middle - 1] as f64 + ordered[middle] as f64) / 2.0
    };
    let abnormal_steps = steps
        .iter()
        .filter(|&&step| step <= 0 || (step as f64) > median_step * 20.0)
        .count() as i32;
    let abnormal_ratio = abnormal_steps as f64 * 100.0 / steps.len() as f64;
    let mut issue_codes = Vec::new();
    if first_pts == last_pts {
        issue_codes.push("PTS_NOT_ADVANCING".to_string());
    }
    if abnormal_ratio > 1.0 {
        issue_codes.push("PTS_ABNORMAL_RATIO".to_string());
    }
    PtsStatistics {
        first_pts,
        last_pts,
        minimum_step: *steps.iter().min().expect("non-empty steps"),
        maximum_step: *steps.iter().max().expect("non-empty steps"),
        negative_steps: steps.iter().filter(|&&step| step < 0).count() as i32,
        zero_steps: steps.iter().filter(|&&step| step == 0).count() as i32,
        median_step,
        abnormal_steps,
        abnormal_ratio,
        steps,
        issue_codes,
    }
}

pub fn dispatch(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "aob.pts_statistics" => {
            let values = value(&request, "Values")
                .and_then(Value::as_array)
                .ok_or("Missing AOB PTS values")?
                .iter()
                .map(|item| item.as_i64().ok_or("AOB PTS value must be an integer"))
                .collect::<Result<Vec<_>, _>>()?;
            serde_json::to_value(pts_statistics(&values)).map_err(|error| error.to_string())
        }
        _ => Err(format!("Unsupported Rust AOB operation: {operation}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn computes_steps_and_abnormal_ratio() {
        let result = pts_statistics(&[100, 200, 300, 300]);
        assert_eq!(result.first_pts, Some(100));
        assert_eq!(result.last_pts, Some(300));
        assert_eq!(result.steps, vec![100, 100, 0]);
        assert_eq!(result.minimum_step, 0);
        assert_eq!(result.maximum_step, 100);
        assert_eq!(result.zero_steps, 1);
        assert_eq!(result.abnormal_steps, 1);
        assert!(
            result
                .issue_codes
                .contains(&"PTS_ABNORMAL_RATIO".to_string())
        );
    }

    #[test]
    fn reports_too_few_pts() {
        let result = pts_statistics(&[100, 200]);
        assert_eq!(result.issue_codes, vec!["PTS_TOO_FEW"]);
        assert!(result.steps.is_empty());
    }

    #[test]
    fn dispatch_rejects_non_integer_values() {
        let result = dispatch("aob.pts_statistics", json!({"Values": [1, "2", 3]}));
        assert!(result.is_err());
    }
}
