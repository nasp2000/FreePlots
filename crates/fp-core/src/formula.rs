// SPDX-License-Identifier: MIT
//! Spreadsheet formulas for the worksheet cells.
//!
//! A cell accepts `=…` and the result is computed once, when the cell is
//! committed. References use the address shown by the formula bar: `C1R1` is
//! column C1, row 1, and `C1R1:C1R10` is the range down that column.
//!
//! Supported: `+ - * / ^ %`, unary signs, comparisons (`= <> < <= > >=`),
//! parentheses, text in double quotes, and the functions `SUM`, `AVERAGE`,
//! `MIN`, `MAX`, `COUNT`, `COUNTA`, `ABS`, `SQRT`, `ROUND`, `LN`, `LOG10`,
//! `EXP`, `POWER`, `MOD`, `IF` and `CONCAT`.

use crate::data::DataFrame;
use crate::error::{Error, Result};

/// Result of a formula: a number or a text.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Numeric result.
    Number(f64),
    /// Text result.
    Text(String),
}

impl Value {
    /// The value as text, ready to be written into a cell.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Number(value) => format_number(*value),
            Self::Text(text) => text.clone(),
        }
    }

    fn number(&self) -> Result<f64> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Text(text) => Err(Error::Unsupported(format!(
                "\"{text}\" is text, not a number"
            ))),
        }
    }

    fn truthy(&self) -> Result<bool> {
        match self {
            Self::Number(value) => Ok(*value != 0.0),
            Self::Text(text) => Ok(!text.is_empty()),
        }
    }
}

fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "#NUM!".to_owned();
    }
    let mut text = format!("{value:.10}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".to_owned() } else { text }
}

/// Evaluates `=…` for a worksheet.
///
/// # Errors
///
/// Returns a message for a syntax error, an out-of-range reference, a
/// division by zero or an unknown function.
pub fn evaluate(df: &DataFrame, text: &str) -> Result<Value> {
    let body = text.trim();
    let body = body.strip_prefix('=').unwrap_or(body);
    let mut parser = Parser {
        chars: body.chars().collect(),
        position: 0,
        df,
    };
    let value = parser.comparison()?;
    parser.skip_spaces();
    if parser.position < parser.chars.len() {
        return Err(Error::Unsupported(format!(
            "unexpected \"{}\" in the formula",
            parser.rest()
        )));
    }
    Ok(value)
}

struct Parser<'a> {
    chars: Vec<char>,
    position: usize,
    df: &'a DataFrame,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.position).copied()
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.position += 1;
        }
    }

    fn eat(&mut self, expected: char) -> bool {
        self.skip_spaces();
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn rest(&self) -> String {
        self.chars[self.position..].iter().collect()
    }

    fn comparison(&mut self) -> Result<Value> {
        let mut left = self.additive()?;
        loop {
            self.skip_spaces();
            let Some(operator) = self.comparison_operator() else {
                return Ok(left);
            };
            let right = self.additive()?;
            let a = left.number()?;
            let b = right.number()?;
            let outcome = match operator.as_str() {
                "=" => a == b,
                "<>" => a != b,
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                ">=" => a >= b,
                _ => false,
            };
            left = Value::Number(if outcome { 1.0 } else { 0.0 });
        }
    }

    fn comparison_operator(&mut self) -> Option<String> {
        for operator in ["<=", ">=", "<>", "=", "<", ">"] {
            let mut matched = true;
            for (offset, expected) in operator.chars().enumerate() {
                if self.chars.get(self.position + offset) != Some(&expected) {
                    matched = false;
                    break;
                }
            }
            if matched {
                self.position += operator.len();
                return Some(operator.to_owned());
            }
        }
        None
    }

    fn additive(&mut self) -> Result<Value> {
        let mut value = self.multiplicative()?;
        loop {
            self.skip_spaces();
            let operator = match self.peek() {
                Some('+') => '+',
                Some('-') => '-',
                _ => return Ok(value),
            };
            self.position += 1;
            let right = self.multiplicative()?;
            let a = value.number()?;
            let b = right.number()?;
            value = Value::Number(if operator == '+' { a + b } else { a - b });
        }
    }

    fn multiplicative(&mut self) -> Result<Value> {
        let mut value = self.power()?;
        loop {
            self.skip_spaces();
            let operator = match self.peek() {
                Some('*') => '*',
                Some('/') => '/',
                Some('%') => '%',
                _ => return Ok(value),
            };
            self.position += 1;
            let right = self.power()?;
            let a = value.number()?;
            let b = right.number()?;
            value = Value::Number(match operator {
                '*' => a * b,
                '/' => {
                    if b == 0.0 {
                        return Err(Error::Unsupported("division by zero".to_owned()));
                    }
                    a / b
                }
                _ => {
                    if b == 0.0 {
                        return Err(Error::Unsupported("division by zero".to_owned()));
                    }
                    a % b
                }
            });
        }
    }

    fn unary(&mut self) -> Result<Value> {
        self.skip_spaces();
        match self.peek() {
            Some('-') => {
                self.position += 1;
                Ok(Value::Number(-self.unary()?.number()?))
            }
            Some('+') => {
                self.position += 1;
                self.unary()
            }
            _ => self.primary(),
        }
    }

    fn power(&mut self) -> Result<Value> {
        let base = self.unary()?;
        self.skip_spaces();
        if self.eat('^') {
            let exponent = self.power()?;
            return Ok(Value::Number(base.number()?.powf(exponent.number()?)));
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<Value> {
        self.skip_spaces();
        match self.peek() {
            Some('(') => {
                self.position += 1;
                let value = self.comparison()?;
                if !self.eat(')') {
                    return Err(Error::Unsupported(
                        "missing a closing parenthesis".to_owned(),
                    ));
                }
                Ok(value)
            }
            Some('"') => self.text(),
            Some(character) if character.is_ascii_digit() || character == '.' => self.number(),
            Some(character) if character.is_alphabetic() => self.name(),
            Some(character) => Err(Error::Unsupported(format!(
                "unexpected \"{character}\" in the formula"
            ))),
            None => Err(Error::Unsupported("the formula is incomplete".to_owned())),
        }
    }

    fn number(&mut self) -> Result<Value> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_digit() || character == '.')
        {
            self.position += 1;
        }
        if self.peek() == Some('e') || self.peek() == Some('E') {
            let save = self.position;
            self.position += 1;
            if self.peek() == Some('-') || self.peek() == Some('+') {
                self.position += 1;
            }
            if self
                .peek()
                .is_some_and(|character| character.is_ascii_digit())
            {
                while self
                    .peek()
                    .is_some_and(|character| character.is_ascii_digit())
                {
                    self.position += 1;
                }
            } else {
                self.position = save;
            }
        }
        let literal: String = self.chars[start..self.position].iter().collect();
        literal
            .parse::<f64>()
            .map(Value::Number)
            .map_err(|_| Error::Unsupported(format!("\"{literal}\" is not a number")))
    }

    fn text(&mut self) -> Result<Value> {
        self.position += 1; // opening quote
        let mut text = String::new();
        while let Some(character) = self.peek() {
            self.position += 1;
            if character == '"' {
                return Ok(Value::Text(text));
            }
            text.push(character);
        }
        Err(Error::Unsupported("missing a closing quote".to_owned()))
    }

    fn name(&mut self) -> Result<Value> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|character| character.is_alphanumeric() || character == '_')
        {
            self.position += 1;
        }
        let name: String = self.chars[start..self.position].iter().collect();
        self.skip_spaces();
        if self.peek() == Some('(') {
            self.position += 1;
            return self.function(&name.to_ascii_uppercase());
        }
        self.reference(&name)
    }

    fn reference(&self, name: &str) -> Result<Value> {
        let Some((column, row)) = parse_cell(name) else {
            return Err(Error::Unsupported(format!(
                "\"{name}\" is not a cell (write C1R1) or a function"
            )));
        };
        Ok(self.cell(column, row)?.unwrap_or(Value::Number(0.0)))
    }

    fn cell(&self, column: usize, row: usize) -> Result<Option<Value>> {
        if column >= self.df.n_columns() {
            return Err(Error::Unsupported(format!(
                "C{} is outside the worksheet",
                column + 1
            )));
        }
        if row >= self.df.n_rows() {
            return Ok(None);
        }
        let column = &self.df.columns()[column];
        Ok(match column.dtype() {
            crate::data::DType::Text => column.label(row).map(Value::Text),
            _ => column.value(row).map(Value::Number),
        })
    }

    /// A range `CxRy:CxRy` at the start of a function argument, expanded into
    /// the numeric values it contains (blanks and text skipped).
    fn try_range(&mut self) -> Result<Option<Vec<f64>>> {
        let save = self.position;
        let start = self.position;
        while self
            .peek()
            .is_some_and(|character| character.is_alphanumeric())
        {
            self.position += 1;
        }
        let first: String = self.chars[start..self.position].iter().collect();
        self.skip_spaces();
        if self.peek() != Some(':') {
            self.position = save;
            return Ok(None);
        }
        self.position += 1;
        self.skip_spaces();
        let start = self.position;
        while self
            .peek()
            .is_some_and(|character| character.is_alphanumeric())
        {
            self.position += 1;
        }
        let second: String = self.chars[start..self.position].iter().collect();
        let (Some((c1, r1)), Some((c2, r2))) = (parse_cell(&first), parse_cell(&second)) else {
            return Err(Error::Unsupported(
                "a range must look like C1R1:C1R10".to_owned(),
            ));
        };
        let mut values = Vec::new();
        for column in c1..=c2 {
            for row in r1..=r2 {
                if let Some(Value::Number(value)) = self.cell(column, row)? {
                    values.push(value);
                }
            }
        }
        Ok(Some(values))
    }

    fn function(&mut self, name: &str) -> Result<Value> {
        let mut arguments: Vec<Value> = Vec::new();
        let mut ranges: Vec<Vec<f64>> = Vec::new();
        if !self.eat(')') {
            loop {
                if let Some(range) = self.try_range()? {
                    ranges.push(range);
                    arguments.push(Value::Number(f64::NAN));
                } else {
                    arguments.push(self.comparison()?);
                }
                if self.eat(',') {
                    continue;
                }
                if self.eat(')') {
                    break;
                }
                return Err(Error::Unsupported(format!(
                    "missing a comma or a closing parenthesis in {name}"
                )));
            }
        }
        let numbers: Vec<f64> = ranges
            .iter()
            .flatten()
            .copied()
            .chain(arguments.iter().filter_map(|value| match value {
                Value::Number(number) if number.is_finite() => Some(*number),
                _ => None,
            }))
            .collect();
        let count = numbers.len() as f64;
        let aggregate = |function: fn(&[f64]) -> f64| {
            if numbers.is_empty() {
                Err(Error::Unsupported(format!(
                    "{name} needs at least one number"
                )))
            } else {
                Ok(Value::Number(function(&numbers)))
            }
        };
        match name {
            "SUM" => aggregate(|values| values.iter().sum()),
            "AVERAGE" | "AVG" => {
                aggregate(|values| values.iter().sum::<f64>() / values.len() as f64)
            }
            "MIN" => aggregate(|values| values.iter().copied().fold(f64::INFINITY, f64::min)),
            "MAX" => aggregate(|values| values.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
            "COUNT" => Ok(Value::Number(count)),
            "COUNTA" => Ok(Value::Number(
                count
                    + arguments
                        .iter()
                        .filter(|value| !matches!(value, Value::Number(_)))
                        .count() as f64,
            )),
            "ABS" => self
                .single(name, &arguments)
                .map(|value| Value::Number(value.abs())),
            "SQRT" => self.single(name, &arguments).and_then(|value| {
                if value < 0.0 {
                    Err(Error::Unsupported(
                        "SQRT needs a non-negative number".to_owned(),
                    ))
                } else {
                    Ok(Value::Number(value.sqrt()))
                }
            }),
            "LN" => self.single(name, &arguments).and_then(|value| {
                if value <= 0.0 {
                    Err(Error::Unsupported("LN needs a positive number".to_owned()))
                } else {
                    Ok(Value::Number(value.ln()))
                }
            }),
            "LOG10" => self.single(name, &arguments).and_then(|value| {
                if value <= 0.0 {
                    Err(Error::Unsupported(
                        "LOG10 needs a positive number".to_owned(),
                    ))
                } else {
                    Ok(Value::Number(value.log10()))
                }
            }),
            "EXP" => self
                .single(name, &arguments)
                .map(|value| Value::Number(value.exp())),
            "ROUND" => {
                if arguments.len() != 2 {
                    return Err(Error::Unsupported(
                        "ROUND needs a number and the digits".to_owned(),
                    ));
                }
                let value = arguments[0].number()?;
                let digits = arguments[1].number()?.round().clamp(0.0, 15.0) as i32;
                let factor = 10f64.powi(digits);
                Ok(Value::Number((value * factor).round() / factor))
            }
            "POWER" => {
                if arguments.len() != 2 {
                    return Err(Error::Unsupported(
                        "POWER needs a base and an exponent".to_owned(),
                    ));
                }
                Ok(Value::Number(
                    arguments[0].number()?.powf(arguments[1].number()?),
                ))
            }
            "MOD" => {
                if arguments.len() != 2 {
                    return Err(Error::Unsupported("MOD needs two numbers".to_owned()));
                }
                let divisor = arguments[1].number()?;
                if divisor == 0.0 {
                    return Err(Error::Unsupported("division by zero".to_owned()));
                }
                Ok(Value::Number(arguments[0].number()? % divisor))
            }
            "IF" => {
                if arguments.len() != 3 {
                    return Err(Error::Unsupported(
                        "IF needs a condition, the true value and the false value".to_owned(),
                    ));
                }
                if arguments[0].truthy()? {
                    Ok(arguments[1].clone())
                } else {
                    Ok(arguments[2].clone())
                }
            }
            "CONCAT" => Ok(Value::Text(
                arguments
                    .iter()
                    .map(Value::text)
                    .collect::<Vec<_>>()
                    .join(""),
            )),
            other => Err(Error::Unsupported(format!("unknown function {other}"))),
        }
    }

    fn single(&self, name: &str, arguments: &[Value]) -> Result<f64> {
        match arguments {
            [value] => value.number(),
            _ => Err(Error::Unsupported(format!(
                "{name} needs exactly one number"
            ))),
        }
    }
}

/// Parses `C<column>R<row>` into zero-based indexes.
fn parse_cell(name: &str) -> Option<(usize, usize)> {
    let upper = name.to_ascii_uppercase();
    let rest = upper.strip_prefix('C')?;
    let split = rest.find('R')?;
    let column = rest[..split].parse::<usize>().ok()?;
    let row = rest[split + 1..].parse::<usize>().ok()?;
    if column == 0 || row == 0 {
        return None;
    }
    Some((column - 1, row - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text(
                "b",
                vec![
                    "x".to_owned(),
                    "y".to_owned(),
                    "z".to_owned(),
                    "w".to_owned(),
                ],
            ),
        ])
        .unwrap()
    }

    fn number(text: &str) -> f64 {
        match evaluate(&frame(), text).expect("formula") {
            Value::Number(value) => value,
            Value::Text(text) => panic!("text {text}"),
        }
    }

    #[test]
    fn arithmetic_follows_the_excel_precedence() {
        assert_eq!(number("=1+2*3"), 7.0);
        assert_eq!(number("=(1+2)*3"), 9.0);
        assert_eq!(number("=-2^2"), 4.0);
        assert_eq!(number("=2^3^2"), 512.0);
        assert_eq!(number("=10/4"), 2.5);
        assert_eq!(number("=7%4"), 3.0);
        assert_eq!(number("=1.5e2"), 150.0);
    }

    #[test]
    fn ranges_and_aggregates_read_the_worksheet() {
        assert_eq!(number("=C1R1"), 1.0);
        assert_eq!(number("=C1R1+C1R2"), 3.0);
        assert_eq!(number("=SUM(C1R1:C1R4)"), 10.0);
        assert_eq!(number("=AVERAGE(C1R1:C1R4)"), 2.5);
        assert_eq!(number("=MIN(C1R1:C1R4)"), 1.0);
        assert_eq!(number("=MAX(C1R1:C1R4)"), 4.0);
        assert_eq!(number("=COUNT(C1R1:C1R4)"), 4.0);
        // A blank reference is zero, a blank cell inside a range is skipped.
        assert_eq!(number("=C1R99"), 0.0);
        assert_eq!(number("=SUM(C1R1:C2R2)"), 3.0);
    }

    #[test]
    fn text_and_logical_functions_work() {
        let frame = frame();
        assert_eq!(
            evaluate(&frame, "=IF(C1R3>2,\"big\",\"small\")"),
            Ok(Value::Text("big".to_owned()))
        );
        assert_eq!(
            evaluate(&frame, "=CONCAT(\"a\",\"-\",C2R1)"),
            Ok(Value::Text("a-x".to_owned()))
        );
        assert_eq!(number("=ROUND(2.345,2)"), 2.35);
        assert_eq!(number("=SQRT(9)"), 3.0);
        assert_eq!(number("=POWER(2,10)"), 1024.0);
        assert_eq!(number("=MOD(7,3)"), 1.0);
        assert_eq!(number("=IF(C1R1=1,10,20)"), 10.0);
    }

    #[test]
    fn invalid_formulas_are_reported() {
        let frame = frame();
        for formula in [
            "=SUM(",
            "=1/0",
            "=C99R1",
            "=C1R1+",
            "=UNKNOWN(1)",
            "=1+*2",
            "=\"unterminated",
        ] {
            assert!(evaluate(&frame, formula).is_err(), "{formula}");
        }
    }
}
