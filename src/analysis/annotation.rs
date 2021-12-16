use crate::clarity::representations::Span;
use regex::Regex;

#[derive(Debug)]
pub enum AnnotationKind {
    Allow(WarningKind),
}

impl std::str::FromStr for AnnotationKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"([[:word:]]+)(\(([^)]+)\))?").unwrap();
        if let Some(captures) = re.captures(s) {
            let (base, value) = if captures.get(1).is_some() && captures.get(3).is_some() {
                (&captures[1], &captures[3])
            } else {
                (&captures[1], "")
            };
            match base {
                "allow" => match value.parse() {
                    Ok(value) => Ok(AnnotationKind::Allow(value)),
                    Err(e) => Err("missing value for 'allow' annotation".to_string()),
                },
                _ => Err("unrecognized annotation".to_string()),
            }
        } else {
            Err("malformed annotation".to_string())
        }
    }
}

#[derive(Debug)]
pub enum WarningKind {
    UncheckedData,
    UncheckedParams,
}

impl std::str::FromStr for WarningKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unchecked_data" => Ok(WarningKind::UncheckedData),
            "unchecked_params" => Ok(WarningKind::UncheckedParams),
            _ => Err(format!("'{}' is not a valid warning identifier", s)),
        }
    }
}

#[derive(Debug)]
pub struct Annotation {
    pub kind: AnnotationKind,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unchecked_data() {
        match "unchecked_data".parse::<WarningKind>() {
            Ok(WarningKind::UncheckedData) => (),
            _ => panic!("failed to parse warning kind correectly"),
        };
    }

    #[test]
    fn parse_warning_error() {
        match "invalid_string".parse::<WarningKind>() {
            Err(_) => (),
            _ => panic!("failed to return error for bad string"),
        };
    }

    #[test]
    fn parse_allow_unchecked_data() {
        match "allow(unchecked_data)".parse::<AnnotationKind>() {
            Ok(AnnotationKind::Allow(WarningKind::UncheckedData)) => (),
            _ => panic!("failed to parse annotation kind correectly"),
        };
    }

    #[test]
    fn parse_annotation_kind_error() {
        match "invalid_string".parse::<AnnotationKind>() {
            Err(_) => (),
            _ => panic!("failed to return error for bad string"),
        };
    }

    #[test]
    fn parse_annotation_kind_error2() {
        match "invalid(string)".parse::<AnnotationKind>() {
            Err(_) => (),
            _ => panic!("failed to return error for bad string"),
        };
    }

    #[test]
    fn parse_annotation_kind_empty() {
        match "".parse::<AnnotationKind>() {
            Err(_) => (),
            _ => panic!("failed to return error for bad string"),
        };
    }
}
