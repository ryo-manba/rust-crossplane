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
    char: String,
    line: usize,
}

#[derive(Debug, PartialEq)]
struct TokenLine {
    value: &'static str,
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

    while let Some(mut cl) = it.next() {
        // handle whitespace
        if cl.char.trim().is_empty() {
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

            while let Some(next_cl) = it.peek() {
                if !next_cl.char.trim().is_empty() {
                    break;
                }
                it.next();
            }
            continue;
        }

        // if starting comment
        if token.is_empty() && cl.char == "#" {
            let line_at_start = cl.line;
            token += &cl.char;

            for next_cl in it.by_ref() {
                if next_cl.char != "\n" {
                    token += &next_cl.char;
                } else {
                    break;
                }
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

        // handle parameter expansion syntax (ex: "${var[@]}")s
        if !token.is_empty() && token.ends_with('$') && cl.char == "{" {
            token += &cl.char;

            for next_cl in it.by_ref() {
                if !token.ends_with('}') && !next_cl.char.trim().is_empty() {
                    token.push_str(&next_cl.char);
                } else {
                    cl = next_cl;
                    break;
                }
            }
        }

        // if a quote is found, add the whole string to the token buffer
        if cl.char == "\"" || cl.char == "'" {
            // if a quote is inside a token, treat it like any other char
            if !token.is_empty() {
                token += &cl.char;
                continue;
            }

            let quote = &cl.char;
            for inner_cl in &mut it {
                if inner_cl.char == *quote {
                    break;
                }

                if inner_cl.char == "\\".to_owned() + quote {
                    token += quote;
                } else {
                    token += &inner_cl.char;
                }
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
        if cl.char == "{" || cl.char == "}" || cl.char == ";" {
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
                value: cl.char.clone(),
                line: cl.line,
                is_quoted: false,
                error: None,
            });
            continue;
        }

        // append char to the token buffer
        token += &cl.char;
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

fn read_chars<R: Read>(mut reader: R) -> impl Iterator<Item = String> {
    let mut buffer = String::new();
    reader.read_to_string(&mut buffer).unwrap();
    buffer
        .chars()
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

fn line_count(chars: impl Iterator<Item = String>) -> impl Iterator<Item = CharLine> {
    let mut line = 1;
    chars.map(move |ch| {
        if ch == "\n" {
            line += 1;
        }
        CharLine { char: ch, line }
    })
}

fn escape_chars(chars: impl Iterator<Item = String>) -> impl Iterator<Item = String> {
    let mut chars = chars.peekable();
    std::iter::from_fn(move || {
        while let Some(ch) = chars.next() {
            if ch == "\\" {
                match chars.peek() {
                    Some(next_char) if next_char == "\n" => {
                        return None;
                    }
                    Some(_) => {
                        return Some(ch + &chars.next().unwrap_or_default());
                    }
                    None => {
                        return Some(ch);
                    }
                }
            } else if ch == "\r" || ch == "\\\r" {
                continue;
            } else {
                return Some(ch);
            }
        }
        None
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
        let fixtures = vec![
            LexFixture {
                name: "simple",
                tokens: vec![
                    TokenLine {
                        value: "events",
                        line: 1,
                    },
                    TokenLine {
                        value: "{",
                        line: 1,
                    },
                    TokenLine {
                        value: "worker_connections",
                        line: 2,
                    },
                    TokenLine {
                        value: "1024",
                        line: 2,
                    },
                    TokenLine {
                        value: ";",
                        line: 2,
                    },
                    TokenLine {
                        value: "}",
                        line: 3,
                    },
                    TokenLine {
                        value: "http",
                        line: 5,
                    },
                    TokenLine {
                        value: "{",
                        line: 5,
                    },
                    TokenLine {
                        value: "server",
                        line: 6,
                    },
                    TokenLine {
                        value: "{",
                        line: 6,
                    },
                    TokenLine {
                        value: "listen",
                        line: 7,
                    },
                    TokenLine {
                        value: "127.0.0.1:8080",
                        line: 7,
                    },
                    TokenLine {
                        value: ";",
                        line: 7,
                    },
                    TokenLine {
                        value: "server_name",
                        line: 8,
                    },
                    TokenLine {
                        value: "default_server",
                        line: 8,
                    },
                    TokenLine {
                        value: ";",
                        line: 8,
                    },
                    TokenLine {
                        value: "location",
                        line: 9,
                    },
                    TokenLine {
                        value: "/",
                        line: 9,
                    },
                    TokenLine {
                        value: "{",
                        line: 9,
                    },
                    TokenLine {
                        value: "return",
                        line: 10,
                    },
                    TokenLine {
                        value: "200",
                        line: 10,
                    },
                    TokenLine {
                        value: "foo bar baz",
                        line: 10,
                    },
                    TokenLine {
                        value: ";",
                        line: 10,
                    },
                    TokenLine {
                        value: "}",
                        line: 11,
                    },
                    TokenLine {
                        value: "}",
                        line: 12,
                    },
                    TokenLine {
                        value: "}",
                        line: 13,
                    },
                ],
            },
            LexFixture {
                name: "with-comments",
                tokens: vec![
                    TokenLine {
                        value: "events",
                        line: 1,
                    },
                    TokenLine {
                        value: "{",
                        line: 1,
                    },
                    TokenLine {
                        value: "worker_connections",
                        line: 2,
                    },
                    TokenLine {
                        value: "1024",
                        line: 2,
                    },
                    TokenLine {
                        value: ";",
                        line: 2,
                    },
                    TokenLine {
                        value: "}",
                        line: 3,
                    },
                    TokenLine {
                        value: "#comment",
                        line: 4,
                    },
                    TokenLine {
                        value: "http",
                        line: 5,
                    },
                    TokenLine {
                        value: "{",
                        line: 5,
                    },
                    TokenLine {
                        value: "server",
                        line: 6,
                    },
                    TokenLine {
                        value: "{",
                        line: 6,
                    },
                    TokenLine {
                        value: "listen",
                        line: 7,
                    },
                    TokenLine {
                        value: "127.0.0.1:8080",
                        line: 7,
                    },
                    TokenLine {
                        value: ";",
                        line: 7,
                    },
                    TokenLine {
                        value: "#listen",
                        line: 7,
                    },
                    TokenLine {
                        value: "server_name",
                        line: 8,
                    },
                    TokenLine {
                        value: "default_server",
                        line: 8,
                    },
                    TokenLine {
                        value: ";",
                        line: 8,
                    },
                    TokenLine {
                        value: "location",
                        line: 9,
                    },
                    TokenLine {
                        value: "/",
                        line: 9,
                    },
                    TokenLine {
                        value: "{",
                        line: 9,
                    },
                    TokenLine {
                        value: "## this is brace",
                        line: 9,
                    },
                    TokenLine {
                        value: "# location /",
                        line: 10,
                    },
                    TokenLine {
                        value: "return",
                        line: 11,
                    },
                    TokenLine {
                        value: "200",
                        line: 11,
                    },
                    TokenLine {
                        value: "foo bar baz",
                        line: 11,
                    },
                    TokenLine {
                        value: ";",
                        line: 11,
                    },
                    TokenLine {
                        value: "}",
                        line: 12,
                    },
                    TokenLine {
                        value: "}",
                        line: 13,
                    },
                    TokenLine {
                        value: "}",
                        line: 14,
                    },
                ],
            },
            LexFixture {
                name: "messy",
                tokens: vec![
                    TokenLine {
                        value: "user",
                        line: 1,
                    },
                    TokenLine {
                        value: "nobody",
                        line: 1,
                    },
                    TokenLine {
                        value: ";",
                        line: 1,
                    },
                    TokenLine {
                        value: "# hello\\n\\\\n\\\\\\n worlddd  \\#\\\\#\\\\\\# dfsf\\n \\\\n \\\\\\n ",
                        line: 2,
                    },
                    TokenLine {
                        value: "events",
                        line: 3,
                    },
                    TokenLine {
                        value: "{",
                        line: 3,
                    },
                    TokenLine {
                        value: "worker_connections",
                        line: 3,
                    },
                    TokenLine {
                        value: "2048",
                        line: 3,
                    },
                    TokenLine {
                        value: ";",
                        line: 3,
                    },
                    TokenLine {
                        value: "}",
                        line: 3,
                    },
                    TokenLine {
                        value: "http",
                        line: 5,
                    },
                    TokenLine {
                        value: "{",
                        line: 5,
                    },
                    TokenLine {
                        value: "#forteen",
                        line: 5,
                    },
                    TokenLine {
                        value: "# this is a comment",
                        line: 6,
                    },
                    TokenLine {
                        value: "access_log",
                        line: 7,
                    },
                    TokenLine {
                        value: "off",
                        line: 7,
                    },
                    TokenLine {
                        value: ";",
                        line: 7,
                    },
                    TokenLine {
                        value: "default_type",
                        line: 7,
                    },
                    TokenLine {
                        value: "text/plain",
                        line: 7,
                    },
                    TokenLine {
                        value: ";",
                        line: 7,
                    },
                    TokenLine {
                        value: "error_log",
                        line: 7,
                    },
                    TokenLine {
                        value: "off",
                        line: 7,
                    },
                    TokenLine {
                        value: ";",
                        line: 7,
                    },
                    TokenLine {
                        value: "server",
                        line: 8,
                    },
                    TokenLine {
                        value: "{",
                        line: 8,
                    },
                    TokenLine {
                        value: "listen",
                        line: 9,
                    },
                    TokenLine {
                        value: "8083",
                        line: 9,
                    },
                    TokenLine {
                        value: ";",
                        line: 9,
                    },
                    TokenLine {
                        value: "return",
                        line: 10,
                    },
                    TokenLine {
                        value: "200",
                        line: 10,
                    },
                    TokenLine {
                        value: "Ser\" ' ' ver\\\\ \\ $server_addr:\\$server_port\\n\\nTime: $time_local\\n\\n",
                        line: 10,
                    },
                    TokenLine {
                        value: ";",
                        line: 10,
                    },
                    TokenLine {
                        value: "}",
                        line: 11,
                    },
                    TokenLine {
                        value: "server",
                        line: 12,
                    },
                    TokenLine {
                        value: "{",
                        line: 12,
                    },
                    TokenLine {
                        value: "listen",
                        line: 12,
                    },
                    TokenLine {
                        value: "8080",
                        line: 12,
                    },
                    TokenLine {
                        value: ";",
                        line: 12,
                    },
                    TokenLine {
                        value: "root",
                        line: 13,
                    },
                    TokenLine {
                        value: "/usr/share/nginx/html",
                        line: 13,
                    },
                    TokenLine {
                        value: ";",
                        line: 13,
                    },
                    TokenLine {
                        value: "location",
                        line: 14,
                    },
                    TokenLine {
                        value: "~",
                        line: 14,
                    },
                    TokenLine {
                        value: "/hello/world;",
                        line: 14,
                    },
                    TokenLine {
                        value: "{",
                        line: 14,
                    },
                    TokenLine {
                        value: "return",
                        line: 14,
                    },
                    TokenLine {
                        value: "301",
                        line: 14,
                    },
                    TokenLine {
                        value: "/status.html",
                        line: 14,
                    },
                    TokenLine {
                        value: ";",
                        line: 14,
                    },
                    TokenLine {
                        value: "}",
                        line: 14,
                    },
                    TokenLine {
                        value: "location",
                        line: 15,
                    },
                    TokenLine {
                        value: "/foo",
                        line: 15,
                    },
                    TokenLine {
                        value: "{",
                        line: 15,
                    },
                    TokenLine {
                        value: "}",
                        line: 15,
                    },
                    TokenLine {
                        value: "location",
                        line: 15,
                    },
                    TokenLine {
                        value: "/bar",
                        line: 15,
                    },
                    TokenLine {
                        value: "{",
                        line: 15,
                    },
                    TokenLine {
                        value: "}",
                        line: 15,
                    },
                    TokenLine {
                        value: "location",
                        line: 16,
                    },
                    TokenLine {
                        value: "/\\{\\;\\}\\ #\\ ab",
                        line: 16,
                    },
                    TokenLine {
                        value: "{",
                        line: 16,
                    },
                    TokenLine {
                        value: "}",
                        line: 16,
                    },
                    TokenLine {
                        value: "# hello",
                        line: 16,
                    },
                    TokenLine {
                        value: "if",
                        line: 17,
                    },
                    TokenLine {
                        value: "($request_method",
                        line: 17,
                    },
                    TokenLine {
                        value: "=",
                        line: 17,
                    },
                    TokenLine {
                        value: "P\\{O\\)\\###\\;ST",
                        line: 17,
                    },
                    TokenLine {
                        value: ")",
                        line: 17,
                    },
                    TokenLine {
                        value: "{",
                        line: 17,
                    },
                    TokenLine {
                        value: "}",
                        line: 17,
                    },
                    TokenLine {
                        value: "location",
                        line: 18,
                    },
                    TokenLine {
                        value: "/status.html",
                        line: 18,
                    },
                    TokenLine {
                        value: "{",
                        line: 18,
                    },
                    TokenLine {
                        value: "try_files",
                        line: 19,
                    },
                    TokenLine {
                        value: "/abc/${uri} /abc/${uri}.html",
                        line: 19,
                    },
                    TokenLine {
                        value: "=404",
                        line: 19,
                    },
                    TokenLine {
                        value: ";",
                        line: 19,
                    },
                    TokenLine {
                        value: "}",
                        line: 20,
                    },
                    TokenLine {
                        value: "location",
                        line: 21,
                    },
                    TokenLine {
                        value: "/sta;\n                    tus",
                        line: 21,
                    },
                    TokenLine {
                        value: "{",
                        line: 22,
                    },
                    TokenLine {
                        value: "return",
                        line: 22,
                    },
                    TokenLine {
                        value: "302",
                        line: 22,
                    },
                    TokenLine {
                        value: "/status.html",
                        line: 22,
                    },
                    TokenLine {
                        value: ";",
                        line: 22,
                    },
                    TokenLine {
                        value: "}",
                        line: 22,
                    },
                    TokenLine {
                        value: "location",
                        line: 23,
                    },
                    TokenLine {
                        value: "/upstream_conf",
                        line: 23,
                    },
                    TokenLine {
                        value: "{",
                        line: 23,
                    },
                    TokenLine {
                        value: "return",
                        line: 23,
                    },
                    TokenLine {
                        value: "200",
                        line: 23,
                    },
                    TokenLine {
                        value: "/status.html",
                        line: 23,
                    },
                    TokenLine {
                        value: ";",
                        line: 23,
                    },
                    TokenLine {
                        value: "}",
                        line: 23,
                    },
                    TokenLine {
                        value: "}",
                        line: 23,
                    },
                    TokenLine {
                        value: "server",
                        line: 24,
                    },
                    TokenLine {
                        value: "{",
                        line: 25,
                    },
                    TokenLine {
                        value: "}",
                        line: 25,
                    },
                    TokenLine {
                        value: "}",
                        line: 25,
                    },
                ],
            },
        ];

        for fixture in fixtures {
            let dirname = Path::new("configs").join(fixture.name);
            let config = dirname.join("nginx.conf");

            let content = fs::read_to_string(&config).expect("Failed to read config");
            let tokens = lex(content.as_bytes());

            println!("Running test: {}", fixture.name);
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
