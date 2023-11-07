use std::fs;
use std::io::Read;

#[derive(Debug, PartialEq)]
pub struct NgxToken {
    value: String,
    line: usize,
    is_quoted: bool,
    error: Option<ParseError>,
}

#[derive(Debug, PartialEq)]
struct ParseError {
    what: String,
    line: usize,
}

struct CharLine {
    char: char,
    line: usize,
}

#[derive(Debug, PartialEq)]
struct TokenLine {
    value: String,
    line: usize,
}

struct LexFixture {
    name: &'static str,
    tokens: Vec<TokenLine>,
}

pub fn lex<R: Read>(reader: R) -> Vec<NgxToken> {
    balance_braces(tokenize(reader))
}

fn balance_braces(tokens: Vec<NgxToken>) -> Vec<NgxToken> {
    let mut balanced_tokens = Vec::new();
    let mut depth = 0;
    let mut line = 0;

    for token in tokens {
        line = token.line;

        if token.value == "}" && !token.is_quoted {
            depth -= 1;
        } else if token.value == "{" && !token.is_quoted {
            depth += 1;
        }

        if depth < 0 {
            return vec![NgxToken {
                value: String::new(),
                line,
                is_quoted: false,
                error: Some(ParseError {
                    what: "unexpected '}'".to_string(),
                    line,
                }),
            }];
        }
        balanced_tokens.push(token);
    }

    if depth > 0 {
        balanced_tokens.push(NgxToken {
            value: String::new(),
            line,
            is_quoted: false,
            error: Some(ParseError {
                what: "unexpected end of file, expecting '}'".to_string(),
                line,
            }),
        });
    }

    balanced_tokens
}

fn tokenize<R: Read>(reader: R) -> Vec<NgxToken> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut token_line = 1;

    let mut it = line_count(escape_chars(read_chars(reader))).peekable();

    while let Some(cl) = it.next() {
        // handle whitespace
        if cl.char.is_whitespace() {
            // if token complete yield it and reset token buffer
            if !token.is_empty() {
                println!("{}", token.clone());

                tokens.push(NgxToken {
                    value: token.clone(),
                    line: token_line,
                    is_quoted: false,
                    error: None,
                });
                token.clear();
            }

            while let Some(next_cl) = it.peek() {
                if !next_cl.char.is_whitespace() {
                    break;
                }
                it.next();
            }
            continue;
        }

        // if starting comment
        if token.is_empty() && cl.char == '#' {
            let line_at_start = cl.line;
            while it.peek().map_or(false, |next_cl| next_cl.char != '\n') {
                token.push(cl.char);
                it.next();
            }
            tokens.push(NgxToken {
                value: token.clone(),
                line: line_at_start,
                is_quoted: false,
                error: None,
            });
            token.clear();
            continue;
        }

        if token.is_empty() {
            token_line = cl.line;
        }

        // handle parameter expansion syntax (ex: "${var[@]}")
        if !token.is_empty() && token.ends_with('$') && cl.char == '{' {
            while let Some(next_cl) = it.peek() {
                if next_cl.char == '}' || next_cl.char.is_whitespace() {
                    break;
                }
                let next_ch = it.next().unwrap().char;
                token.push(next_ch);
            }
        }

        // if a quote is found, add the whole string to the token buffer
        if cl.char == '"' || cl.char == '\'' {
            // if a quote is inside a token, treat it like any other char
            if !token.is_empty() {
                token.push(cl.char);
                continue;
            }

            let quote = cl.char;
            while let Some(inner_cl) = it.next() {
                if inner_cl.char == '\\' {
                    if let Some(escaped_char) = it.next() {
                        if escaped_char.char == quote {
                            token.push(quote);
                            continue;
                        } else {
                            token.push('\\');
                            token.push(escaped_char.char);
                            continue;
                        }
                    } else {
                        break;
                    }
                }
                if inner_cl.char == quote {
                    break;
                }
                token.push(inner_cl.char);
            }

            tokens.push(NgxToken {
                value: token.clone(),
                line: token_line,
                is_quoted: true,
                error: None,
            });
            token.clear();
            continue;
        }

        // handle special characters that are treated like full tokens
        if cl.char == '{' || cl.char == '}' || cl.char == ';' {
            // if token complete yield it and reset token buffer
            if !token.is_empty() {
                tokens.push(NgxToken {
                    value: token.clone(),
                    line: token_line,
                    is_quoted: false,
                    error: None,
                });
                token.clear();
            }

            // this character is a full token so yield it now
            tokens.push(NgxToken {
                value: cl.char.to_string(),
                line: cl.line,
                is_quoted: false,
                error: None,
            });
            continue;
        }

        // append char to the token buffer
        token.push(cl.char);
    }

    if !token.is_empty() {
        tokens.push(NgxToken {
            value: token.clone(),
            line: token_line,
            is_quoted: false,
            error: None,
        });
    }

    tokens
}

fn read_chars<R: Read>(mut reader: R) -> impl Iterator<Item = char> {
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer).unwrap();
    buffer.chars().collect::<Vec<_>>().into_iter()
}

fn line_count(chars: impl Iterator<Item = char>) -> impl Iterator<Item = CharLine> {
    let mut line = 1;
    chars.map(move |ch| {
        if ch == '\n' {
            line += 1;
        }
        CharLine { char: ch, line }
    })
}

fn escape_chars(chars: impl Iterator<Item = char>) -> impl Iterator<Item = char> {
    chars.flat_map(|ch| {
        if ch == '\\' {
            Some(ch)
        } else if ch == '\r' {
            None
        } else {
            Some(ch)
        }
    })
}

impl PartialEq<TokenLine> for NgxToken {
    fn eq(&self, other: &TokenLine) -> bool {
        self.value == other.value && self.line == other.line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_lex() {
        let fixtures = vec![LexFixture {
            name: "simple",
            tokens: vec![
                TokenLine {
                    value: "events".to_string(),
                    line: 1,
                },
                TokenLine {
                    value: "{".to_string(),
                    line: 1,
                },
                TokenLine {
                    value: "worker_connections".to_string(),
                    line: 2,
                },
                TokenLine {
                    value: "1024".to_string(),
                    line: 2,
                },
                TokenLine {
                    value: ";".to_string(),
                    line: 2,
                },
                TokenLine {
                    value: "}".to_string(),
                    line: 3,
                },
                TokenLine {
                    value: "http".to_string(),
                    line: 5,
                },
                TokenLine {
                    value: "{".to_string(),
                    line: 5,
                },
                TokenLine {
                    value: "server".to_string(),
                    line: 6,
                },
                TokenLine {
                    value: "{".to_string(),
                    line: 6,
                },
                TokenLine {
                    value: "listen".to_string(),
                    line: 7,
                },
                TokenLine {
                    value: "127.0.0.1:8080".to_string(),
                    line: 7,
                },
                TokenLine {
                    value: ";".to_string(),
                    line: 7,
                },
                TokenLine {
                    value: "server_name".to_string(),
                    line: 8,
                },
                TokenLine {
                    value: "default_server".to_string(),
                    line: 8,
                },
                TokenLine {
                    value: ";".to_string(),
                    line: 8,
                },
                TokenLine {
                    value: "location".to_string(),
                    line: 9,
                },
                TokenLine {
                    value: "/".to_string(),
                    line: 9,
                },
                TokenLine {
                    value: "{".to_string(),
                    line: 9,
                },
                TokenLine {
                    value: "return".to_string(),
                    line: 10,
                },
                TokenLine {
                    value: "200".to_string(),
                    line: 10,
                },
                TokenLine {
                    value: "foo bar baz".to_string(),
                    line: 10,
                },
                TokenLine {
                    value: ";".to_string(),
                    line: 10,
                },
                TokenLine {
                    value: "}".to_string(),
                    line: 11,
                },
                TokenLine {
                    value: "}".to_string(),
                    line: 12,
                },
                TokenLine {
                    value: "}".to_string(),
                    line: 13,
                },
            ],
        }];

        for fixture in fixtures {
            let dirname = Path::new("configs").join(fixture.name);
            let config = dirname.join("nginx.conf");

            let content = fs::read_to_string(&config).expect("Failed to read config");
            let tokens = lex(content.as_bytes());

            for (i, token) in tokens.iter().enumerate() {
                let expected = &fixture.tokens[i];
                if token.value != expected.value || token.line != expected.line {
                    panic!(
                        "expected ({:?}, {:?}) but got ({:?}, {:?})",
                        expected.value, expected.line, token.value, token.line
                    );
                }
            }
        }
    }
}
