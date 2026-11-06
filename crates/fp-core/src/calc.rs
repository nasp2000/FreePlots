//! Column calculator: a small arithmetic language over the worksheet.
//!
//! An expression combines numbers, column references and functions with the
//! usual precedence (`^` binds tighter than `*` and `/`, which bind tighter
//! than `+` and `-`), parentheses and a unary minus. Column references are
//! written as `[weight]` (any name, including spaces) or as a bare identifier
//! (`weight`, `C1`).
//!
//! Functions: `sqrt`, `ln`, `log10`, `exp`, `abs`, `round`, `floor`, `ceil`,
//! `min(a, b)`, `max(a, b)` and `pow(a, b)`.
//!
//! A row where any referenced cell is missing produces a missing value in the
//! result; a row where the arithmetic is undefined (square root of a negative
//! number, logarithm of zero, division by zero, overflow) is reported as an
//! error with the row number, so a bad formula never fills a column with
//! silent garbage.
//!
//! The documented behaviour has a worksheet calculator; its syntax was not
//! documented, so the grammar here is our own — see `the project notes`.

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};

/// One token of the expression.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Identifier(String),
    Column(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Open,
    Close,
    Comma,
}

/// Splits the text into tokens.
fn tokenize(text: &str) -> Result<Vec<Token>> {
    let characters: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < characters.len() {
        let character = characters[index];
        match character {
            ' ' | '\t' | '\n' | '\r' => index += 1,
            '+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                index += 1;
            }
            '(' => {
                tokens.push(Token::Open);
                index += 1;
            }
            ')' => {
                tokens.push(Token::Close);
                index += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            '[' => {
                let start = index + 1;
                let Some(end) = characters[start..].iter().position(|value| *value == ']') else {
                    return Err(Error::Unsupported(
                        "the column reference is missing its closing bracket".to_owned(),
                    ));
                };
                let name: String = characters[start..start + end].iter().collect();
                if name.trim().is_empty() {
                    return Err(Error::Unsupported(
                        "the column reference is empty".to_owned(),
                    ));
                }
                tokens.push(Token::Column(name.trim().to_owned()));
                index = start + end + 1;
            }
            digit if digit.is_ascii_digit() || digit == '.' => {
                let start = index;
                while index < characters.len()
                    && (characters[index].is_ascii_digit() || characters[index] == '.')
                {
                    index += 1;
                }
                // Scientific notation is accepted after the mantissa.
                if index < characters.len()
                    && (characters[index] == 'e' || characters[index] == 'E')
                {
                    let mut look = index + 1;
                    if look < characters.len()
                        && (characters[look] == '+' || characters[look] == '-')
                    {
                        look += 1;
                    }
                    if look < characters.len() && characters[look].is_ascii_digit() {
                        index = look;
                        while index < characters.len() && characters[index].is_ascii_digit() {
                            index += 1;
                        }
                    }
                }
                let text: String = characters[start..index].iter().collect();
                let value = text.parse::<f64>().map_err(|_| {
                    Error::Unsupported(format!("the number {text} could not be read"))
                })?;
                tokens.push(Token::Number(value));
            }
            letter if letter.is_alphabetic() || letter == '_' => {
                let start = index;
                while index < characters.len()
                    && (characters[index].is_alphanumeric() || characters[index] == '_')
                {
                    index += 1;
                }
                let text: String = characters[start..index].iter().collect();
                tokens.push(Token::Identifier(text));
            }
            other => {
                return Err(Error::Unsupported(format!(
                    "the character {other:?} is not part of an expression"
                )));
            }
        }
    }
    Ok(tokens)
}

/// A node of the parsed expression.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Number(f64),
    Column(String),
    Negate(Box<Node>),
    Binary(BinOp, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

/// Recursive-descent parser over the token list.
struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn expect(&mut self, token: Token) -> Result<()> {
        match self.next() {
            Some(found) if found == token => Ok(()),
            _ => Err(Error::Unsupported(
                "the expression is incomplete".to_owned(),
            )),
        }
    }

    /// `expression := term (('+' | '-') term)*`
    fn expression(&mut self) -> Result<Node> {
        let mut node = self.term()?;
        while let Some(token) = self.peek() {
            let op = match token {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Subtract,
                _ => break,
            };
            self.next();
            let right = self.term()?;
            node = Node::Binary(op, Box::new(node), Box::new(right));
        }
        Ok(node)
    }

    /// `term := power (('*' | '/') power)*`
    fn term(&mut self) -> Result<Node> {
        let mut node = self.power()?;
        while let Some(token) = self.peek() {
            let op = match token {
                Token::Star => BinOp::Multiply,
                Token::Slash => BinOp::Divide,
                _ => break,
            };
            self.next();
            let right = self.power()?;
            node = Node::Binary(op, Box::new(node), Box::new(right));
        }
        Ok(node)
    }

    /// `power := unary ('^' power)?` — right associative.
    fn power(&mut self) -> Result<Node> {
        let base = self.unary()?;
        if self.peek() == Some(&Token::Caret) {
            self.next();
            let exponent = self.power()?;
            return Ok(Node::Binary(
                BinOp::Power,
                Box::new(base),
                Box::new(exponent),
            ));
        }
        Ok(base)
    }

    /// `unary := '-' unary | atom`
    fn unary(&mut self) -> Result<Node> {
        if self.peek() == Some(&Token::Minus) {
            self.next();
            let inner = self.unary()?;
            return Ok(Node::Negate(Box::new(inner)));
        }
        self.atom()
    }

    /// `atom := number | column | identifier('(' arguments ')') | '(' expression ')'`
    fn atom(&mut self) -> Result<Node> {
        match self.next() {
            Some(Token::Number(value)) => Ok(Node::Number(value)),
            Some(Token::Column(name)) => Ok(Node::Column(name)),
            Some(Token::Identifier(name)) => {
                if self.peek() == Some(&Token::Open) {
                    self.next();
                    let mut arguments = Vec::new();
                    if self.peek() != Some(&Token::Close) {
                        loop {
                            arguments.push(self.expression()?);
                            match self.peek() {
                                Some(Token::Comma) => {
                                    self.next();
                                }
                                _ => break,
                            }
                        }
                    }
                    self.expect(Token::Close)?;
                    Ok(Node::Call(name, arguments))
                } else {
                    Ok(Node::Column(name))
                }
            }
            Some(Token::Open) => {
                let node = self.expression()?;
                self.expect(Token::Close)?;
                Ok(node)
            }
            _ => Err(Error::Unsupported(
                "the expression is incomplete".to_owned(),
            )),
        }
    }
}

/// Parses an expression.
fn parse(text: &str) -> Result<Node> {
    let tokens = tokenize(text)?;
    if tokens.is_empty() {
        return Err(Error::Unsupported("the expression is empty".to_owned()));
    }
    let mut parser = Parser {
        tokens,
        position: 0,
    };
    let node = parser.expression()?;
    if parser.position != parser.tokens.len() {
        return Err(Error::Unsupported(
            "the expression has extra characters at the end".to_owned(),
        ));
    }
    Ok(node)
}

/// Columns referenced by an expression, in order of appearance.
pub fn references(text: &str) -> Result<Vec<String>> {
    let node = parse(text)?;
    let mut names = Vec::new();
    collect(&node, &mut names);
    Ok(names)
}

fn collect(node: &Node, names: &mut Vec<String>) {
    match node {
        Node::Number(_) => {}
        Node::Column(name) => {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        Node::Negate(inner) => collect(inner, names),
        Node::Binary(_, left, right) => {
            collect(left, names);
            collect(right, names);
        }
        Node::Call(_, arguments) => {
            for argument in arguments {
                collect(argument, names);
            }
        }
    }
}

/// Evaluates the expression for every row of the worksheet.
///
/// Rows with a missing input produce `NaN`; an undefined operation is an error
/// naming the row.
pub fn evaluate(df: &DataFrame, text: &str) -> Result<Vec<f64>> {
    let node = parse(text)?;
    let mut columns = Vec::new();
    for name in references(text)? {
        let column = df.column(&name)?;
        columns.push((name, row_values(column)?));
    }
    let mut values = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let mut missing = false;
        for (_, series) in &columns {
            if !series[row].is_finite() {
                missing = true;
                break;
            }
        }
        if missing {
            values.push(f64::NAN);
            continue;
        }
        let mut lookup = |name: &str| -> f64 {
            columns
                .iter()
                .find(|(column, _)| column == name)
                .map_or(f64::NAN, |(_, series)| series[row])
        };
        match eval(&node, &mut lookup) {
            Ok(value) if value.is_finite() => values.push(value),
            Ok(_) => {
                return Err(Error::Unsupported(format!(
                    "the expression is not finite on row {}",
                    row + 1
                )));
            }
            Err(detail) => {
                return Err(Error::Unsupported(format!("row {}: {detail}", row + 1)));
            }
        }
    }
    Ok(values)
}

/// Values of a column aligned with the rows, `NaN` where the cell is missing.
fn row_values(column: &Column) -> Result<Vec<f64>> {
    let mut values = Vec::with_capacity(column.len());
    for row in 0..column.len() {
        if column.is_missing(row) {
            values.push(f64::NAN);
            continue;
        }
        match column.data() {
            crate::data::ColumnData::F64(series) => values.push(series[row]),
            crate::data::ColumnData::I64(series) => values.push(series[row] as f64),
            crate::data::ColumnData::Date(series) => values.push(series[row] as f64),
            crate::data::ColumnData::Bool(series) => {
                values.push(if series[row] { 1.0 } else { 0.0 });
            }
            crate::data::ColumnData::Text(_) => {
                return Err(Error::Unsupported(format!(
                    "the column {} is not numeric",
                    column.name()
                )));
            }
        }
    }
    Ok(values)
}

fn eval(node: &Node, lookup: &mut dyn FnMut(&str) -> f64) -> Result<f64> {
    Ok(match node {
        Node::Number(value) => *value,
        Node::Column(name) => lookup(name),
        Node::Negate(inner) => -eval(inner, lookup)?,
        Node::Binary(op, left, right) => {
            let a = eval(left, lookup)?;
            let b = eval(right, lookup)?;
            match op {
                BinOp::Add => a + b,
                BinOp::Subtract => a - b,
                BinOp::Multiply => a * b,
                BinOp::Divide => {
                    if b == 0.0 {
                        return Err(Error::Unsupported("division by zero".to_owned()));
                    }
                    a / b
                }
                BinOp::Power => a.powf(b),
            }
        }
        Node::Call(name, arguments) => {
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments {
                values.push(eval(argument, lookup)?);
            }
            let arity = |expected: usize| -> Result<()> {
                if values.len() == expected {
                    Ok(())
                } else {
                    Err(Error::Unsupported(format!(
                        "{name} needs {expected} argument(s)"
                    )))
                }
            };
            match name.as_str() {
                "sqrt" => {
                    arity(1)?;
                    if values[0] < 0.0 {
                        return Err(Error::Unsupported("sqrt of a negative value".to_owned()));
                    }
                    values[0].sqrt()
                }
                "ln" => {
                    arity(1)?;
                    if values[0] <= 0.0 {
                        return Err(Error::Unsupported("ln of a non-positive value".to_owned()));
                    }
                    values[0].ln()
                }
                "log10" => {
                    arity(1)?;
                    if values[0] <= 0.0 {
                        return Err(Error::Unsupported(
                            "log10 of a non-positive value".to_owned(),
                        ));
                    }
                    values[0].log10()
                }
                "exp" => {
                    arity(1)?;
                    values[0].exp()
                }
                "abs" => {
                    arity(1)?;
                    values[0].abs()
                }
                "round" => {
                    arity(1)?;
                    values[0].round()
                }
                "floor" => {
                    arity(1)?;
                    values[0].floor()
                }
                "ceil" => {
                    arity(1)?;
                    values[0].ceil()
                }
                "min" => {
                    arity(2)?;
                    values[0].min(values[1])
                }
                "max" => {
                    arity(2)?;
                    values[0].max(values[1])
                }
                "pow" => {
                    arity(2)?;
                    values[0].powf(values[1])
                }
                other => {
                    return Err(Error::Unsupported(format!("unknown function {other}")));
                }
            }
        }
    })
}

/// Adds a column with the values of an expression.
pub fn add_column(df: &mut DataFrame, name: &str, text: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Unsupported("the new column needs a name".to_owned()));
    }
    if df.column(name).is_ok() {
        return Err(Error::Unsupported(format!(
            "the worksheet already has a column named {name}"
        )));
    }
    let values = evaluate(df, text)?;
    let column = Column::new_f64(name, values);
    let index = df.n_columns();
    df.insert_column(index, column)?;
    Ok(())
}
