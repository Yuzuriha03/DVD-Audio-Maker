//! Navigation VM commands used by the menu authoring path.
//!
//! Slice 1 of the navigation migration: lexer, parser and byte compiler for the
//! VM command language embedded in pre/post/button XML elements. The authoring
//! path emits set/jump/exit/resume statements only, so the compiler accepts that
//! subset and reports every other construct as an explicit error instead of
//! guessing at DVD-Video VM semantics.

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Number(u16),
    Register(u16),
    SystemRegister(u16),
    Identifier(String),
    Keyword(Keyword),
    Symbol(Symbol),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    Amgm,
    And,
    Angle,
    Audio,
    Break,
    Button,
    Call,
    Cell,
    Chapter,
    Counter,
    Else,
    Entry,
    Exit,
    Fpc,
    Goto,
    Group,
    If,
    Jump,
    Menu,
    Next,
    Not,
    Or,
    Pgc,
    Prev,
    Program,
    Ptt,
    Random,
    Region,
    Resume,
    Root,
    Subtitle,
    Tail,
    Title,
    Track,
    Titleset,
    Top,
    Up,
    Vmgm,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    AndAssign,
    OrAssign,
    XorAssign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Ampersand,
    Pipe,
    Caret,
    Eq,
    Ne,
    Ge,
    Gt,
    Le,
    Lt,
    Land,
    Lor,
    Not,
    Semicolon,
    Colon,
    OpenBrace,
    CloseBrace,
    OpenParen,
    CloseParen,
}

fn keyword(text: &str) -> Option<Keyword> {
    Some(match text {
        "amgm" => Keyword::Amgm,
        "and" => Keyword::And,
        "angle" => Keyword::Angle,
        "audio" => Keyword::Audio,
        "break" => Keyword::Break,
        "button" => Keyword::Button,
        "call" => Keyword::Call,
        "cell" => Keyword::Cell,
        "chapter" => Keyword::Chapter,
        "counter" => Keyword::Counter,
        "else" => Keyword::Else,
        "entry" => Keyword::Entry,
        "exit" => Keyword::Exit,
        "fpc" => Keyword::Fpc,
        "goto" => Keyword::Goto,
        "group" => Keyword::Group,
        "if" => Keyword::If,
        "jump" => Keyword::Jump,
        "menu" => Keyword::Menu,
        "next" => Keyword::Next,
        "not" => Keyword::Not,
        "or" => Keyword::Or,
        "pgc" => Keyword::Pgc,
        "prev" => Keyword::Prev,
        "program" => Keyword::Program,
        "ptt" => Keyword::Ptt,
        "random" => Keyword::Random,
        "region" => Keyword::Region,
        "resume" => Keyword::Resume,
        "root" => Keyword::Root,
        "subtitle" => Keyword::Subtitle,
        "tail" => Keyword::Tail,
        "title" => Keyword::Title,
        "track" => Keyword::Track,
        "titleset" => Keyword::Titleset,
        "top" => Keyword::Top,
        "up" => Keyword::Up,
        "vmgm" => Keyword::Vmgm,
        "xor" => Keyword::Xor,
        _ => return None,
    })
}

fn lex(text: &str) -> Result<Vec<Token>, String> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            let rest = &text[index + 2..];
            let end = rest.find("*/").ok_or("unterminated VM comment")?;
            index += 2 + end + 2;
            continue;
        }
        if byte.is_ascii_digit() {
            let start = index;
            index += 1;
            if byte == b'0' && matches!(bytes.get(index), Some(b'x' | b'X')) {
                index += 1;
                let digits = index;
                while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
                    index += 1;
                }
                let value = u16::from_str_radix(&text[digits..index], 16)
                    .map_err(|error| format!("bad VM number: {error}"))?;
                tokens.push(Token::Number(value));
                continue;
            }
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            let value: u32 = text[start..index]
                .parse()
                .map_err(|error| format!("bad VM number: {error}"))?;
            let value = if index < bytes.len() && bytes[index] == b'k' {
                index += 1;
                if value >= 64 {
                    return Err(format!("VM number {value}k out of range"));
                }
                value * 1024
            } else {
                value
            };
            if value > u16::MAX as u32 {
                return Err(format!("VM number {value} out of range"));
            }
            tokens.push(Token::Number(value as u16));
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let word = &text[start..index];
            if let Some(register) = word.strip_prefix('g')
                && register.bytes().all(|byte| byte.is_ascii_digit())
            {
                let value: u16 = register
                    .parse()
                    .map_err(|error| format!("bad GPRM: {error}"))?;
                tokens.push(Token::Register(value));
                continue;
            }
            if let Some(register) = word.strip_prefix('s')
                && register.bytes().all(|byte| byte.is_ascii_digit())
            {
                let value: u16 = register
                    .parse()
                    .map_err(|error| format!("bad SPRM: {error}"))?;
                tokens.push(Token::SystemRegister(value));
                continue;
            }
            if let Some(keyword) = keyword(word) {
                tokens.push(Token::Keyword(keyword));
            } else {
                tokens.push(Token::Identifier(word.to_string()));
            }
            continue;
        }
        let (symbol, length) = if index + 1 < bytes.len() {
            match (byte, bytes[index + 1]) {
                (b'=', b'=') => (Symbol::Eq, 2),
                (b'!', b'=') => (Symbol::Ne, 2),
                (b'>', b'=') => (Symbol::Ge, 2),
                (b'<', b'=') => (Symbol::Le, 2),
                (b'&', b'&') => (Symbol::Land, 2),
                (b'|', b'|') => (Symbol::Lor, 2),
                (b'+', b'=') => (Symbol::AddAssign, 2),
                (b'-', b'=') => (Symbol::SubAssign, 2),
                (b'*', b'=') => (Symbol::MulAssign, 2),
                (b'/', b'=') => (Symbol::DivAssign, 2),
                (b'%', b'=') => (Symbol::ModAssign, 2),
                (b'&', b'=') => (Symbol::AndAssign, 2),
                (b'|', b'=') => (Symbol::OrAssign, 2),
                (b'^', b'=') => (Symbol::XorAssign, 2),
                _ => (
                    single_symbol(byte)
                        .ok_or_else(|| format!("bad VM character {:?}", byte as char))?,
                    1,
                ),
            }
        } else {
            (
                single_symbol(byte)
                    .ok_or_else(|| format!("bad VM character {:?}", byte as char))?,
                1,
            )
        };
        tokens.push(Token::Symbol(symbol));
        index += length;
    }
    Ok(tokens)
}

fn single_symbol(byte: u8) -> Option<Symbol> {
    Some(match byte {
        b'=' => Symbol::Assign,
        b'+' => Symbol::Plus,
        b'-' => Symbol::Minus,
        b'*' => Symbol::Star,
        b'/' => Symbol::Slash,
        b'%' => Symbol::Percent,
        b'&' => Symbol::Ampersand,
        b'|' => Symbol::Pipe,
        b'^' => Symbol::Caret,
        b'>' => Symbol::Gt,
        b'<' => Symbol::Lt,
        b'!' => Symbol::Not,
        b';' => Symbol::Semicolon,
        b':' => Symbol::Colon,
        b'{' => Symbol::OpenBrace,
        b'}' => Symbol::CloseBrace,
        b'(' => Symbol::OpenParen,
        b')' => Symbol::CloseParen,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Statement {
    Set { register: u16, value: u16 },
    JumpMenu(u16),
    JumpTrack { group: u16, track: u16 },
    Exit,
    Resume,
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned();
        if token.is_some() {
            self.index += 1;
        }
        token
    }

    fn expect_symbol(&mut self, symbol: Symbol) -> Result<(), String> {
        match self.next() {
            Some(Token::Symbol(found)) if found == symbol => Ok(()),
            other => Err(format!("expected {symbol:?}, found {other:?}")),
        }
    }

    fn expect_keyword(&mut self, keyword: Keyword) -> Result<(), String> {
        match self.next() {
            Some(Token::Keyword(found)) if found == keyword => Ok(()),
            other => Err(format!("expected {keyword:?}, found {other:?}")),
        }
    }

    fn number(&mut self) -> Result<u16, String> {
        match self.next() {
            Some(Token::Number(value)) => Ok(value),
            other => Err(format!("expected number, found {other:?}")),
        }
    }

    fn program(&mut self) -> Result<Vec<Statement>, String> {
        let mut statements = Vec::new();
        while self.peek().is_some() {
            statements.push(self.statement()?);
        }
        Ok(statements)
    }

    fn statement(&mut self) -> Result<Statement, String> {
        match self.next() {
            Some(Token::Keyword(Keyword::Exit)) => {
                self.expect_symbol(Symbol::Semicolon)?;
                Ok(Statement::Exit)
            }
            Some(Token::Keyword(Keyword::Resume)) => {
                self.expect_symbol(Symbol::Semicolon)?;
                Ok(Statement::Resume)
            }
            Some(Token::Keyword(Keyword::Jump)) => self.jump(),
            Some(Token::Register(register)) => self.set(register),
            Some(Token::SystemRegister(_register)) => {
                Err("system register assignments are not emitted by the menu author".into())
            }
            other => Err(format!("unsupported VM statement starting with {other:?}")),
        }
    }

    fn jump(&mut self) -> Result<Statement, String> {
        match self.peek() {
            Some(Token::Keyword(Keyword::Menu)) => {
                self.index += 1;
                let menu = self.number()?;
                self.expect_symbol(Symbol::Semicolon)?;
                Ok(Statement::JumpMenu(menu))
            }
            Some(Token::Keyword(Keyword::Group)) => {
                self.index += 1;
                let group = self.number()?;
                self.expect_keyword(Keyword::Track)?;
                let track = self.number()?;
                self.expect_symbol(Symbol::Semicolon)?;
                Ok(Statement::JumpTrack { group, track })
            }
            other => Err(format!("unsupported jump form: {other:?}")),
        }
    }

    fn set(&mut self, register: u16) -> Result<Statement, String> {
        self.expect_symbol(Symbol::Assign)?;
        let value = match self.next() {
            Some(Token::Number(value)) => value,
            other => return Err(format!("unsupported set value: {other:?}")),
        };
        self.expect_symbol(Symbol::Semicolon)?;
        Ok(Statement::Set { register, value })
    }
}

/// Context the compiler needs to encode AMGM menu targets.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VmContext {
    pub(crate) menu_pgc_count: u16,
}

/// Compiles VM statements into 8-byte DVD virtual machine instructions.
pub(crate) fn compile(statements: &[Statement], context: VmContext) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(statements.len() * 8);
    for statement in statements {
        let instruction = match statement {
            Statement::Set { register, value } => {
                if *register > 15 {
                    return Err(format!("GPRM {register} out of range"));
                }
                [
                    0x71,
                    0x00,
                    0x00,
                    *register as u8,
                    0x00,
                    *value as u8,
                    0x00,
                    0x00,
                ]
            }
            Statement::JumpMenu(menu) => {
                if *menu == 0 || *menu > context.menu_pgc_count {
                    return Err(format!(
                        "jump menu {menu}: only {} menu PGCs exist",
                        context.menu_pgc_count
                    ));
                }
                if *menu >= 120 {
                    return Err("menu entry jumps are not supported yet".into());
                }
                [0x20, 0x04, 0x00, 0x00, 0x00, 0xc0, 0x00, *menu as u8]
            }
            Statement::JumpTrack { group, track } => [
                0x30,
                0x0a,
                0x00,
                *group as u8,
                0x00,
                *track as u8,
                0x00,
                0x00,
            ],
            Statement::Exit => [0x30, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            Statement::Resume => [0x20, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16],
        };
        bytes.extend_from_slice(&instruction);
    }
    Ok(bytes)
}

/// Parses and compiles a complete command program.
pub(crate) fn compile_text(text: &str, context: VmContext) -> Result<Vec<u8>, String> {
    let tokens = lex(text)?;
    let statements = Parser { tokens, index: 0 }.program()?;
    compile(&statements, context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_handles_comments_registers_and_hex() {
        let tokens = lex("g0=0x10; /* comment */ jump menu 2;").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Register(0),
                Token::Symbol(Symbol::Assign),
                Token::Number(16),
                Token::Symbol(Symbol::Semicolon),
                Token::Keyword(Keyword::Jump),
                Token::Keyword(Keyword::Menu),
                Token::Number(2),
                Token::Symbol(Symbol::Semicolon),
            ]
        );
    }

    #[test]
    fn compiles_the_authoring_subset() {
        let context = VmContext { menu_pgc_count: 3 };
        assert_eq!(
            compile_text("g0=1;", context).unwrap(),
            [0x71, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]
        );
        assert_eq!(
            compile_text("jump menu 1;", context).unwrap(),
            [0x20, 0x04, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x01]
        );
        assert_eq!(
            compile_text("jump group 2 track 3;", context).unwrap(),
            [0x30, 0x0a, 0x00, 0x02, 0x00, 0x03, 0x00, 0x00]
        );
        assert_eq!(
            compile_text("exit;", context).unwrap(),
            [0x30, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            compile_text("resume;", context).unwrap(),
            [0x20, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16]
        );
    }

    #[test]
    fn rejects_unsupported_constructs() {
        let context = VmContext { menu_pgc_count: 1 };
        assert!(compile_text("if (g0 == 1) exit;", context).is_err());
        assert!(compile_text("jump menu 0;", context).is_err());
        assert!(compile_text("jump menu 2;", context).is_err());
        assert!(compile_text("call menu 1;", context).is_err());
        assert!(compile_text("g0 = g1;", context).is_err());
        assert!(compile_text("g0", context).is_err());
    }
}
