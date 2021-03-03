// The goal is for this project to be less than 800 loc in length (not including
// std/builtin.zf and tests).

use std::collections::HashMap;
use std::io::{self, Read};
use std::rc::Rc;

mod errors;
mod stdlib;
mod random;

const NONSYMB: [char; 18] = [ '{', '}', '(', ')', '[', ']',
    '"', ' ', '\t', '\n', '\r', '\\', ';', '&', '#', '$', '@', '!' ];

#[derive(Clone, Debug)]
pub enum ZfToken {
    Nop,
    Number(f64),
    String(String),
    Symbol(usize),
    SymbRef(usize),
    Address(usize),
}

impl ZfToken {
    fn fmt(&self, e: &ZfEnv) -> String {
        match self {
            ZfToken::Nop        => format!("<nop>"),
            ZfToken::Number(i)  => format!("{}", i),
            ZfToken::String(s)  => format!("{:?}", s),
            ZfToken::Symbol(s)  => format!("<symb {}>", e.dict[*s].0),
            ZfToken::SymbRef(s) => format!("<ref {}>",  e.dict[*s].0),
            ZfToken::Address(i) => format!("<addr {}>", i),
        }
    }
}

impl Into<bool> for &ZfToken {
    fn into(self) -> bool {
        match self {
            ZfToken::Number(n) => if *n == 0_f64 { false } else { true },
            _ => true,
        }
    }
}

fn parse(env: &mut ZfEnv, input: &str, in_def: bool)
    -> Result<(usize, Vec<ZfToken>), String>
{
    fn eat<F>(ch: &[char], mut c: usize, until: F)
        -> (String, usize, bool) where F: Fn(&[char]) -> bool
    {
        let mut buf = String::new();
        let mut early_return = true;

        while c < ch.len() {
            if until(&ch[c..]) {
                early_return = false;
                break;
            } else {
                buf += &format!("{}", ch[c]);
                c += 1;
            }
        }

        (buf, c, early_return)
    }

    let mut labels = HashMap::new();
    let mut toks = Vec::new();
    let chs = input.chars()
        .collect::<Vec<char>>();
    let mut i = 0;

    while i < chs.len() {
        match chs[i] {
            // --- whitespace ---
            ' ' | '\t' | '\r' | '\n' => { i += 1; continue; },

            // --- comments ---
            '(' => {
                let s = eat(&chs, i + 1, |c| c[0] == ')');
                if s.2 { return Err(format!("unmatched (")); }
                i = s.1 + 2;
            },
            '\\' => {
                let s = eat(&chs, i + 2, |c| c[0] == '\n');
                i = s.1 + 1;
            },

            // --- strings ---
            '"' => {
                let s = eat(&chs, i + 1, |c| c[0] == '"');
                if s.2 { return Err(format!("unmatched \"")); }
                i = s.1 + 1;
                toks.push(ZfToken::String(s.0));
            },

            // --- quotes ---
            '[' => {
                let body = parse(env, &input[i + 1..], false)?;
                let _ref = env.addword(random::phrase(), body.1);
                toks.push(ZfToken::SymbRef(_ref));
                i += body.0 + 1;
            },
            ']' => { i += 1; return Ok((i, toks)) },

            ':' if !in_def => {
                i = eat(&chs, i + 1, |c| c[0].is_whitespace()).1;
                let name = eat(&chs, i + 1, |c| NONSYMB.contains(&c[0]));
                i = name.1;
                let body = parse(env, &input[i + 1..], true)?;
                env.addword(name.0, body.1);
                i += body.0 + 1;
            },
            ';' if in_def  => { i += 1; return Ok((i, toks)) },
            ':' if in_def  => return Err(format!("found nested word definitions")),
            ';' if !in_def => return Err(format!("stray ;")),

            '|' if chs.len() > i && !chs[i + 1].is_whitespace() => {
                let n = eat(&chs, i + 1, |c| NONSYMB.contains(&c[0]));
                i = n.1;
                toks.push(ZfToken::Nop);
                labels.insert(format!("|{}", n.0), toks.len() - 1);
            },
            '&' if chs.len() > i && !chs[i + 1].is_whitespace() => {
                let n = eat(&chs, i + 1, |c| NONSYMB.contains(&c[0]));
                i = n.1;
                if labels.contains_key(&n.0) {
                    toks.push(ZfToken::Address(labels[&n.0]));
                } else if env.findword(&n.0).is_some() {
                    toks.push(ZfToken::SymbRef(env.findword(&n.0).unwrap()));
                } else {
                    return Err(format!("Unknown label {}", n.0));
                }
            },

            // syntactic sugar
            '$' if chs.len() > i && !chs[i + 1].is_whitespace() => {
                toks.push(ZfToken::Number(chs[i + 1] as u32 as f64));
                i += 2;
            },
            '\'' => {
                let s = eat(&chs, i + 1, |c| c[0].is_whitespace());
                toks.push(ZfToken::String(s.0));
                i = s.1;
            },

            '!' if chs.len() > i && !chs[i + 1].is_whitespace() => {
                let n = eat(&chs, i + 1, |c| NONSYMB.contains(&c[0]));
                i = n.1;
                toks.push(ZfToken::String(n.0));
                toks.push(ZfToken::Symbol(env.findword("store").unwrap()));
            },
            '@' if chs.len() > i && !chs[i + 1].is_whitespace() => {
                let n = eat(&chs, i + 1, |c| NONSYMB.contains(&c[0]));
                i = n.1;
                toks.push(ZfToken::String(n.0));
                toks.push(ZfToken::Symbol(env.findword("fetch").unwrap()));
            },

            _ => {
                let n = eat(&chs, i, |c| NONSYMB.contains(&c[0]));

                i = n.1;
                match n.0.replace("_", "").parse::<f64>() {
                    Ok(o) =>  toks.push(ZfToken::Number(o)),
                    Err(_) => {
                        match env.findword(&n.0) {
                            Some(i) => toks.push(ZfToken::Symbol(i)),
                            None => return Err(format!("unknown word {}", n.0)),
                        }
                    },
                };
            },
        }
    }

    return Ok((i, toks));
}

// The returned bool tells calling code whether the instruction pointer or return
// stack was modified. If it was not, the calling code will know it's safe to increment
// the IP
type ZfProcFunc = dyn Fn(&mut ZfEnv) -> Result<bool, String>;

#[derive(Clone)]
pub enum ZfProc {
    Builtin(Rc<Box<ZfProcFunc>>),
    User(Vec<ZfToken>),
}

#[derive(Clone, Default)]
pub struct ZfEnv {
    pile: Vec<ZfToken>,
    vars: HashMap<String, ZfToken>,
    dict: Vec<(String, ZfProc)>,
    rs:   Vec<(usize, usize)>,
}

impl ZfEnv {
    pub fn new() -> ZfEnv { Default::default() }

    pub fn findword(&self, name: &str) -> Option<usize> {
        for i in 0..self.dict.len() {
            if self.dict[i].0 == name {
                return Some(i);
            }
        }
        None
    }

    pub fn addword(&mut self, name: String, body: Vec<ZfToken>) -> usize {
        match self.findword(&name) {
            Some(i) => self.dict[i].1 = ZfProc::User(body),
            None => self.dict.push((name.clone(), ZfProc::User(body))),
        }
        self.findword(&name).unwrap()
    }
}

fn run(code: Vec<ZfToken>, env: &mut ZfEnv) -> Result<(), String> {
    let main = env.addword("main".to_owned(), code);

    env.rs.push((main, 0));

    loop {
        if env.rs.len() == 0 { break }

        let mut crs = env.rs.len() - 1;
        let (c_ib, ip) = (env.rs[crs].0, env.rs[crs].1);
        let ib;
        if let ZfProc::User(u) = &env.dict[c_ib].1 {
            ib = u;
        } else { unreachable!() }

        if ip >= ib.len() {
            env.rs.pop();
            if env.rs.len() > 0 {
                crs = env.rs.len() - 1;
                env.rs[crs].1 += 1;
            }
            continue;
        }

        // Debugging stuff. :^)
        //eprintln!("  => {:16} at {} {}", env.dict[c_ib].0, ip, ib[ip].fmt(env));

        match &ib[ip] {
            ZfToken::Nop => (),

            ZfToken::Symbol(s) => {
                match &env.dict[*s].clone().1 {
                    ZfProc::Builtin(b) => match (b)(env) {
                        Ok(co) => if co { continue },
                        Err(e) => return Err(e),
                    },
                    ZfProc::User(_) => {
                        env.rs.push((*s, 0));
                        continue; // don't increment IP below
                    },
                }
            },
            ZfToken::SymbRef(i) => env.pile.push(ZfToken::Symbol(*i)),
            _ => env.pile.push(ib[ip].clone()),
        }

        crs = env.rs.len() - 1;
        env.rs[crs].1 += 1;
    }

    Ok(())
}

fn main() {
    let mut env = ZfEnv::new();

    macro_rules! builtin {
        ($s:expr, $x:path) =>
            (env.dict.push(($s.to_string(),
                ZfProc::Builtin(Rc::new(Box::new($x))))))
    }

    builtin!("fetch",  stdlib::FETCH);
    builtin!("store",  stdlib::STORE);
    builtin!("if",        stdlib::IF);
    builtin!("?ret",    stdlib::CRET);
    builtin!("?jump",  stdlib::CJUMP);
    builtin!("depth",  stdlib::DEPTH);
    builtin!("pick",    stdlib::PICK);
    builtin!("roll",    stdlib::ROLL);
    builtin!("drop",    stdlib::DROP);
    builtin!("not",      stdlib::NOT);
    builtin!("cmp",      stdlib::CMP);
    builtin!("+",       stdlib::PLUS);
    builtin!("-",        stdlib::SUB);
    builtin!("*",        stdlib::MUL);
    builtin!("/mod",    stdlib::DMOD);
    builtin!("band",    stdlib::bAND);
    builtin!("bor",      stdlib::bOR);
    builtin!("bxor",    stdlib::bXOR);
    builtin!("bnot",    stdlib::bNOT);
    builtin!("bshl",     stdlib::SHL);
    builtin!("bshr",     stdlib::SHR);
    builtin!("emit",    stdlib::EMIT);
    builtin!("wait",    stdlib::WAIT);
    builtin!(".S",       stdlib::DBG);
    builtin!(".D",   stdlib::DICTDBG);

    macro_rules! include_zf {
        ($path:expr) =>
            (std::str::from_utf8(include_bytes!($path)).unwrap())
    }

    let stdlib_builtin = include_zf!("std/builtin.zf");
    let stdlib_parsed  = parse(&mut env, stdlib_builtin, false);
    run(stdlib_parsed.unwrap().1, &mut env).unwrap();

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();

    match parse(&mut env, &buffer, false) {
        Ok(zf) => {
            match run(zf.1, &mut env) {
                Ok(()) => (),
                Err(e) => {
                    eprintln!("error: {}", e);
                    errors::stacktrace(&mut env);
                    std::process::exit(1);
                },
            }
        }
        Err(e) => eprintln!("error: {}", e),
    }
}
