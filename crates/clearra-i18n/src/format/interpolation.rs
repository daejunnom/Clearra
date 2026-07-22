#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterpolationValue<'a> {
    key: &'a str,
    value: &'a str,
}

impl<'a> InterpolationValue<'a> {
    pub fn new(key: &'a str, value: &'a str) -> Self {
        Self { key, value }
    }
}

pub fn interpolate(template: &str, values: &[InterpolationValue<'_>]) -> String {
    let mut rendered = template.to_owned();
    for value in values {
        let token = format!("{{{}}}", value.key);
        rendered = rendered.replace(&token, value.value);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_named_tokens_without_touching_unknown_tokens() {
        let rendered = interpolate(
            "Backend {backend} fell back because {reason}. {unknown}",
            &[
                InterpolationValue::new("backend", "gpu"),
                InterpolationValue::new("reason", "gpu_feature_disabled"),
            ],
        );

        assert_eq!(
            rendered,
            "Backend gpu fell back because gpu_feature_disabled. {unknown}"
        );
    }
}
