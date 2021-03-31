// The goal is for this project to be less than 800 loc in length (not including
// std/builtin.zf and tests).

#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate pest;

use std::collections::HashMap;
use std::io::{self, Read};
use std::rc::Rc;

mod ratios;
mod floats;
mod parser;
mod errors;
mod stdlib;
mod random;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GuardItem {
    Any, Number, Str, Quote,
    Unchecked
}

#[derive(Clone, Debug)]
pub enum ZfToken {
    Nop,
    Number(f64),
    String(String),
    Symbol(usize),
    SymbRef(usize),
    Fetch(String),
    Store(String),
    Table(HashMap<ZfToken, ZfToken>),

    Guard {
        before: Vec<GuardItem>,
        after:  Vec<GuardItem>
    },

    // Only used during parsing.
    Ident(String),
}

impl ZfToken {
    fn fmt(&self, e: &ZfEnv) -> String {
        match self {
            ZfToken::Nop        => format!("<nop>"),
            ZfToken::Number(i)  => format!("{}", i),
            ZfToken::String(s)  => format!("{:?}", s),
            ZfToken::Symbol(s)  => format!("<symb {}>", e.dict[*s].0),
            ZfToken::SymbRef(s) => format!("<ref {}>",  e.dict[*s].0),
            ZfToken::Fetch(s)   => format!("<fetch {}>", s),
            ZfToken::Store(s)   => format!("<store {}>", s),
            ZfToken::Table(t)   => format!("{:?}", t),
            ZfToken::Ident(i)   => format!("<ident {}>", i),

            ZfToken::Guard { before: _, after: _ }
                => format!("<guard {:?}>", self),
        }
    }
}

impl Eq for ZfToken {}

impl PartialEq for ZfToken {
    fn eq(&self, rhs: &Self) -> bool {
        use ZfToken::*;

        match (self, rhs) {
            (Number(l),   Number(r)) => l.to_bits() == r.to_bits(),
            (String(l),   String(r)) => l == r,
            (Symbol(l),   Symbol(r)) => l == r,
            (SymbRef(l), SymbRef(r)) => l == r,
            (Fetch(l),     Fetch(r)) => l == r,
            (Store(l),     Store(r)) => l == r,
            (Table(l),     Table(r)) => {
                if l.len() != r.len() { return false; }
                for (k, v) in l {
                    if !r.contains_key(k) || &r[k] != v { return false; }
                }
                for (k, v) in r {
                    if !l.contains_key(k) || &r[k] != v { return false; }
                }

                true
            },
            (Ident(l),     Ident(r)) => l == r,
            (Guard{before: lb, after: la},
                Guard{before: rb, after: ra}) => lb == rb && la == ra,
            _ => false,
        }
    }
}

impl std::hash::Hash for ZfToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            ZfToken::Number(i) => i.to_bits().hash(state),
            ZfToken::String(s) => s.hash(state),
            ZfToken::SymbRef(s) => s.hash(state),
            ZfToken::Table(t) => for (k, v) in t {
                k.hash(state);
                v.hash(state);
            },

            // This method will only be called when using tables in Zf code.
            // Fetch/Store/Guard/Ident/Nop cannot be put into a table, so this
            // path should never be chosen.
            _ => unreachable!(),
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

// The returned bool tells calling code whether the instruction pointer or
// return stack was modified. If it was not, the calling code will know
// it's safe to increment the IP
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
    rs:   Vec<(usize, usize, Vec<ZfToken>)>,
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

    pub fn pushrs(&mut self, funcid: usize, iptr: usize) {
        self.rs.push((funcid, iptr, Vec::new()));
    }
}

fn run(code: Vec<ZfToken>, env: &mut ZfEnv) -> Result<(), String> {
    let main = env.addword("main".to_owned(), code);

    env.pushrs(main, 0);

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

        match &ib[ip] {
            ZfToken::Nop => (),

            ZfToken::Symbol(s) => {
                match &env.dict[*s].clone().1 {
                    ZfProc::Builtin(b) => match (b)(env) {
                        Ok(co) => if co { continue },
                        Err(e) => return Err(e),
                    },
                    ZfProc::User(_) => {
                        env.pushrs(*s, 0);
                        continue; // don't increment IP below
                    },
                }
            },
            ZfToken::SymbRef(i) => env.pile.push(ZfToken::Symbol(*i)),
            ZfToken::Fetch(var) => if env.vars.contains_key(var) {
                env.pile.push(env.vars[var].clone());
            } else {
                return Err(format!("unknown variable {}", var))
            },
            ZfToken::Store(var) => {
                env.vars.insert(var.clone(), match env.pile.pop() {
                    Some(v) => v,
                    None => return Err(format!("stack underflow")),
                });
            },
            ZfToken::Guard { before: _b, after: _a } => (), // TODO
            _ => env.pile.push(ib[ip].clone()),
        }

        crs = env.rs.len() - 1;
        env.rs[crs].1 += 1;
    }

    Ok(())
}

fn main() {
    let mut env = ZfEnv::new();

    macro_rules! keyword {
        ($s:expr, $x:ident) =>
            (env.dict.push(($s.to_string(),
                ZfProc::Builtin(Rc::new(Box::new(stdlib::$x))))))
    }

    keyword!("if",           IF);
    keyword!("again",     AGAIN);
    keyword!("?ret",       CRET);
    keyword!("depth",     DEPTH);
    keyword!("arrange", ARRANGE);
    keyword!("pick",       PICK);
    keyword!("roll",       ROLL);
    keyword!("drop",       DROP);
    keyword!("not",         NOT);
    keyword!("cmp",         CMP);
    keyword!("+",          PLUS);
    keyword!("-",           SUB);
    keyword!("*",           MUL);
    keyword!("/mod",       DMOD);
    keyword!("and",        bAND);
    keyword!("bor",         bOR);
    keyword!("bxor",       bXOR);
    keyword!("bnot",       bNOT);
    keyword!("bshl",        SHL);
    keyword!("bshr",        SHR);
    keyword!("emit",       EMIT);
    keyword!("wait",       WAIT);
    keyword!("push",       PUSH);
    keyword!("pop",         POP);
    keyword!("dbg",         DBG);
    keyword!("ddbg",    DICTDBG);
    keyword!("ceil",       CEIL);
    keyword!("floor",     FLOOR);
    keyword!("atan",       ATAN);

    macro_rules! include_zf {
        ($path:expr) => {
            run(parser::parse(&mut env,
                    std::str::from_utf8(include_bytes!($path)).unwrap()).unwrap(),
                &mut env).unwrap()
        }
    }

    include_zf!("std/builtin.zf");

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();

    let parsed = match parser::parse(&mut env, &buffer) {
        Ok(tokens) => tokens,
        Err(error) => { eprintln!("{}", error); return; },
    };

    match run(parsed, &mut env) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("error: {}", e);
            errors::stacktrace(&mut env);
            std::process::exit(1);
        },
    }
}
