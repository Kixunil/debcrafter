use std::collections::{HashMap, BTreeMap};
use std::borrow::{Borrow, Cow};
use std::fmt;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Component<'a> {
    Constant(&'a str),
    Variable(&'a str),
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum State {
    Init,
    Variable,
    Escaped,
}

#[derive(Debug)]
pub struct Parser<'a> {
    state: State,
    remaining: Option<&'a str>
}

impl<'a> Parser<'a> {
    pub fn vars(self) -> impl 'a + Iterator<Item=&'a str> {
        self.filter_map(|component| match component {
            Component::Variable(var) => Some(var),
            Component::Constant(_) => None,
        })
    }
}

impl<'a> Iterator for Parser<'a> {
    type Item = Component<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let remaining_str: &mut &str = self.remaining.as_mut()?;
        let bracket = remaining_str.find(&['{', '}'] as &[char]);
        let bracket = bracket.map(|pos| {
            let mut chars = remaining_str[pos..].chars();
            let bracket = chars.next().expect("Bug in find or indexing function");
            let following_char = chars.next();
            (pos, bracket, following_char)
        });
        match (self.state, bracket) {
            (State::Init, None) => {
                let res = &**remaining_str;
                self.remaining = None;
                Some(Component::Constant(res))
            },
            (State::Init, Some((pos, '}', Some('}')))) | (State::Init, Some((pos, '{', Some('{')))) => {
                let res = &remaining_str[..pos];
                self.state = State::Escaped;
                self.remaining = Some(&remaining_str[(pos + 1)..]);
                if res.is_empty() {
                    self.next()
                } else {
                    Some(Component::Constant(res))
                }
            },
            (State::Init, Some((_, '}', _))) => {
                panic!("Invalid template: contains extra right bracket");
            },
            (State::Variable, Some((_, '}', Some('}')))) | (State::Variable, Some((_, '{', _))) => {
                panic!("Invalid template: braces not allowed in variables");
            },
            (State::Init, Some((mut pos @ 0, '{', _))) | (State::Variable, Some((mut pos, '}', _))) => {
                if self.state == State::Init {
                    *remaining_str = &remaining_str[1..];
                    pos = remaining_str.find(&['{', '}'] as &[char]).expect("Invalid template: missing closing brace");
                    let mut chars = remaining_str[pos..].chars();
                    if chars.next() == Some('{') || chars.next() == Some('}') {
                        panic!("Braces not allowed in variable names");
                    }
                }
                let res = &remaining_str[..pos];
                self.state = State::Init;
                let remaining = &remaining_str[(pos + 1)..];
                self.remaining = match remaining.is_empty() {
                    true => None,
                    false => Some(remaining),
                };
                Some(Component::Variable(res))
            },
            (State::Init, Some((pos, '{', _))) => {
                let res = &remaining_str[..pos];
                self.state = State::Variable;
                self.remaining = Some(&remaining_str[(pos + 1)..]);
                Some(Component::Constant(res))
            },
            (State::Init, Some((_, _, _))) => {
                panic!("Invalid parser state");
            },
            (State::Variable, None) => {
                panic!("Invalid template: missing closing brace");
            },
            (State::Variable, Some(_)) => {
                panic!("Invalid parser state");
            },
            (State::Escaped, Some((0, _, _))) => {
                let res;
                match remaining_str[1..].find(&['{', '}'] as &[char]) {
                    Some(pos) => {
                        let pos = pos + 1;
                        let following_str = &remaining_str[pos..];
                        let mut chars = following_str.chars();
                        let a = chars.next();
                        let b = chars.next();
                        let c = chars.next();
                        if c == None && a == b {
                            res = &remaining_str[..(pos + 1)];
                            self.remaining = None;
                        } else {
                            res = &remaining_str[..pos];
                            self.remaining = Some(following_str);
                        }
                    },
                    None => {
                        res = &*remaining_str;
                        self.remaining = None;
                    },
                }
                self.state = State::Init;
                Some(Component::Constant(res))
            },
            (State::Escaped, _) => {
                dbg!(self);
                panic!("template parser in invalid state");
            },
        }
    }
}

pub fn parse<'a>(template: &'a str) -> Parser<'a> {
    Parser {
        state: State::Init,
        remaining: Some(template),
    }
}

pub trait Query {
    fn get(&self, key: &str) -> Option<&str>;
}

impl<T: Query> Query for &T {
    fn get(&self, key: &str) -> Option<&str> {
        (*self).get(key)
    }
}

impl<S1, S2> Query for HashMap<S1, S2> where S1: Borrow<str> + Eq + std::hash::Hash, S2: AsRef<str> {
    fn get(&self, key: &str) -> Option<&str> {
        HashMap::get(self, key).map(AsRef::as_ref)
    }
}

impl<S1, S2> Query for BTreeMap<S1, S2> where S1: Borrow<str> + Eq + Ord, S2: AsRef<str> {
    fn get(&self, key: &str) -> Option<&str> {
        BTreeMap::get(self, key).map(AsRef::as_ref)
    }
}
#[cfg(test)]
mod tests {
    use super::Component::{self, *};

    fn check(template: &str, expected: &[Component<'_>]) {
        let components = super::parse(template).collect::<Vec<_>>();
        assert_eq!(&*components, expected);
    }

    macro_rules! test_case {
        ($name:ident, $template:expr $(, $expected:expr)*) => {
            #[test]
            fn $name() {
                check($template, &[$($expected),*]);
            }
        }
    }

    macro_rules! invalid {
        ($name:ident, $template:expr) => {
            #[test]
            #[should_panic]
            fn $name() {
                for _ in super::parse($template) {
                }
            }
        }
    }
    test_case!(empty, "", Constant("")); 
    test_case!(single_constant, "foo", Constant("foo")); 
    test_case!(single_var, "{foo}", Variable("foo")); 
    test_case!(single_escaped, "{{foo}}", Constant("{foo}")); 
    test_case!(var_begin, "{foo}bar", Variable("foo"), Constant("bar")); 
    test_case!(var_end, "bar{foo}", Constant("bar"), Variable("foo")); 
    test_case!(var_middle, "foo{bar}baz", Constant("foo"), Variable("bar"), Constant("baz")); 
    test_case!(consecutive_vars, "{foo}{bar}", Variable("foo"), Variable("bar")); 
    test_case!(host_port, "{foo}:{bar}", Variable("foo"), Constant(":"), Variable("bar")); 
    test_case!(schema_host_port, "x://{foo}:{bar}", Constant("x://"), Variable("foo"), Constant(":"), Variable("bar")); 

    invalid!(unclosed_var_begin, "{foo");
    invalid!(unclosed_var_middle, "foo{bar");
    invalid!(unclosed_var_end, "foo{");
    invalid!(right_bracket_only, "}");
    invalid!(right_bracket_begin, "}foo");
    invalid!(right_bracket_middle, "foo}bar");
    invalid!(right_bracket_end, "foo}");
    invalid!(left_bracket_in_var, "{foo{bar}");
    invalid!(escaped_left_bracket_in_var, "{foo{{bar}");
    invalid!(escaped_right_bracket_in_var, "{foo}}bar}");
}
