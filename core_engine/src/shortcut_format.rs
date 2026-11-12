//! Platform-independent date/time format syntax shared by settings and the future formatter.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateField {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatPart {
    Field(DateField),
    Literal(String),
}

/// Compile only the six documented tokens; no locale or platform format directives.
/// Error offsets count Unicode scalar values in the user's format, starting at zero.
pub fn compile(format: &str) -> Result<Vec<FormatPart>, usize> {
    let chars: Vec<_> = format.chars().collect();
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut index = 0;
    let mut quote_start = None;
    while index < chars.len() {
        let c = chars[index];
        if c == '\'' {
            if chars.get(index + 1) == Some(&'\'') {
                literal.push('\'');
                index += 2;
                continue;
            }
            quote_start = if quote_start.is_some() {
                None
            } else {
                Some(index)
            };
            index += 1;
            continue;
        }
        if quote_start.is_some() || !c.is_ascii_alphabetic() {
            literal.push(c);
            index += 1;
            continue;
        }
        let (width, field) = match c {
            'y' => (4, DateField::Year),
            'M' => (2, DateField::Month),
            'd' => (2, DateField::Day),
            'H' => (2, DateField::Hour),
            'm' => (2, DateField::Minute),
            's' => (2, DateField::Second),
            _ => return Err(index),
        };
        if chars
            .get(index..index + width)
            .is_none_or(|run| run.iter().any(|value| *value != c))
        {
            return Err(index);
        }
        if !literal.is_empty() {
            parts.push(FormatPart::Literal(std::mem::take(&mut literal)));
        }
        parts.push(FormatPart::Field(field));
        index += width;
    }
    if let Some(start) = quote_start {
        return Err(start);
    }
    if !literal.is_empty() {
        parts.push(FormatPart::Literal(literal));
    }
    Ok(parts)
}
