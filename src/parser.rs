// Copyright (c) 2019 Weird Constructor <weirdconstructor@gmail.com>
// This is a part of WLambda. See README.md and COPYING for details.

use crate::vval::VVal;
use crate::vval::Syntax;

/*

Bukclo

vector      := '$[' expr (',' expr)* ','? ']'
map         := '${' expr ':' expr (',' expr ':' expr)* ','? '}'
number      := float | int
string      := '"' ... '"'
none        := "$none" | "$n"
bool        := "$t" | "$f" | "$true" | "$false"
native_keys := "$if" | "$while" | "$break" | "$for"
key         := ':' identifier
primitive   := vector | map | number | key | string | none | bool
var         := identifier

block       := '{' stmt* '}'
func        := block
value       := primitive | func | var | '[' expr ']'
field       := value | identifier
arglist     := '(' (expr (',' expr)*)?)? ')'
field-acc   := value ('.' field)+          // field access
             | value ('.' field)+ '=' expr // assignment
             | value ('.' field)+ arglist  // method/field call
call        := call arglist
             | call '~' expr
             | field-acc
             | call (binop call)+ // where call is limited to arglist and field-acc
             | call call+ // field-acc and others have priority
             | value call+ // this is then always
expr        := call ('|' expr)*
             | value
def         := '!' key? assignment
assignment  := '.' identifier '=' expr
             | '.'? '(' (identifier (',' identifier)*)? ')' = expr
stmt        := def ';'
             | assignment ';'
             | expr ';'
             | ';'

special variables:
    arguments are in '@', first one is in '_' and _1
    others: _1 up to _9

special functions:
    range   - returning a range iterator
    iter    - called on vec/map it returns a collection iterator
    apply fn arg-list - calls fn with arguments from arg-list
                        makes stuff like `apply bla @` possible

pipe:

    fn a b | fn_b c | fn_c
    =>
    fn_c(fn_b (fn a b) c)

    allows:
        iter x | map { * _ 2 } | filter { even? _ }

    => filter({ even? _ }, map({ * _ 2 }, iter(x)))

tilde:

    fn a b ~ fn_b c ~ fn_c

    { * _ 2 } ~ { - _ 2 } ~ { pow _ 10 } ~ 20
    =>
    { * _ 2 }({ - _ 2 }({ pow _ 10 } 20))



dotcall:

    a.b c d
    =>
    [b a] c d

what calling means for primitive types:

    number  => access index in first arg
    key     => access field in first arg
    vec     => call first arg for every element
    map     => call first arg for every kv pair
    #true   => call first arg
    #false  => call second arg
    string  => call first arg for every character

=>

    let x = 0

    while { < x 10 } {
        x = + x 1;
        [== 0 $ % x 2] { break }
    }

    [range 0 10 1] {
        print "foo {}" _;
    }

    let doit = {
       let (a, b) = @;
       assert = _ a;
    }


Thoughts about cyclic referencing

    let new = {
        ! self = ${ // self on stack
            x: 10
        };
        ! y = $[1,2,3]; // y on stack
        ! :ref yhard = $[1,2,3]; // yhard puts upvalue on stack, any closure captures the upvalue by value so it references it strongly

        self.foo = {
            # think of this as:
            # let self  = shallow copy of outer_self;
            # let y     = shallow copy of outer_y;
            # yhard references yhard up value;

            # or: set! self :x ~ + 1 ~ :x self
            # or: mut! self :x { + _ 1 }; // self captures by value
            self->x = + self->x 1;
            y     = + y 1;      // y captured by value locally no change to outer y
            yhard = + yhard 1;  // y referenced strong now!
        };

        self.bar = {
            + [0 y] [0 yhard]
            # or:
            +(y->0, yhard->0)
            # or:
            + ~ y->0 ~ yhard->0
        }

        self
    }

    let obj = new(); // self is on stack here
    obj.foo;

Callable objects:

    let my_cond = {
        let :ref self = ${ inner_val: #t };
        { apply self->inner_val @ }
    }()

    my_cond { # if
        println "INNER VALUE IS TRUE!"
    } { # else
        println "INNER VALUE IS FALSE!"
    }
*/

#[allow(dead_code)]
pub struct ParseState {
//    contents:   String,
    chars:      Vec<char>,
    peek_char:  char,
    line_no:    i32,
    file:       String,
    at_eof:     bool,
}

#[allow(dead_code)]
impl ParseState {
    pub fn peek(&self) -> Option<char> { if self.at_eof { None } else { Some(self.peek_char) } }

    pub fn peek2(&self) -> Option<String> {
        if self.chars.len() > 1 {
            let s : String = self.chars[0..2].iter().collect();
            Some(s)
        } else {
            None
        }
    }

    pub fn peek_op(&self) -> Option<String> {
        if self.at_eof { return None; }
        match self.peek_char {
            '+' | '-' | '*' | '/' | '%' | '^'
                => { return Some(self.peek_char.to_string()); },
            '<' | '>' | '!' | '=' | '|' | '&' => {
                if let Some(s) = self.peek2() {
                    match &s[0..2] {
                          "<=" | ">=" | "!=" | "==" | "<<" | ">>"
                        | "&&" | "||" | "&|" | "&^" => { return Some(s); }
                        _ => { }
                    }
                }
                if self.peek_char != '=' && self.peek_char != '|' {
                    Some(self.peek_char.to_string())
                } else {
                    None
                }
            },
            _ => { None }
        }
    }

    pub fn rest(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn expect_char(&mut self, expected_char: char) -> bool {
        if let Some(c) = self.peek() {
            if c == expected_char {
                self.consume();
                return true;
            }
        }

        false
    }

    pub fn consume_while<F>(&mut self, pred: F) -> bool
        where F: Fn(char) -> bool {

        let mut did_match_once = false;
        while let Some(c) = self.peek() {
            if pred(c) { self.consume(); did_match_once = true; }
            else { break; }
        }
        did_match_once
    }

    pub fn consume_if_eq_wsc(&mut self, expected_char: char) -> bool {
        let res = self.consume_if_eq(expected_char);
        self.skip_ws_and_comments();
        res
    }

    pub fn consume_if_eq(&mut self, expected_char: char) -> bool {
        if let Some(c) = self.peek() {
            if c == expected_char {
                self.consume();
                return true;
            }
        }
        false
    }

    pub fn take_while_wsc<F>(&mut self, pred: F) -> Vec<char>
        where F: Fn(char) -> bool {
        let ret = self.take_while(pred);
        self.skip_ws_and_comments();
        ret
    }

    pub fn take_while<F>(&mut self, pred: F) -> Vec<char>
        where F: Fn(char) -> bool {

        let mut ret = Vec::new();
        while let Some(c) = self.peek() {
            if !pred(c) { break; }
            ret.push(c);
            self.consume();
        }
        ret
    }

    pub fn consume_lookahead(&mut self, s: &str) -> bool {
        if self.lookahead(s) {
            for _ in s.chars() { self.chars.remove(0); }
            if self.chars.len() > 0 {
                self.peek_char = self.chars[0];
            } else {
                self.peek_char = ' ';
                self.at_eof = true;
            }
            return true;
        }
        false
    }

    pub fn lookahead_one_of(&self, s: &str) -> bool {
        if self.at_eof { return false; }

        for c in s.chars() {
            if self.peek_char == c {
                return true;
            }
        }
        return false;
    }

    pub fn lookahead(&mut self, s: &str) -> bool {
        if self.chars.len() < s.len() {
            return false;
        }

        let mut i = 0;
        for c in s.chars() {
            if self.chars[i] != c {
                return false;
            }
            i = i + 1;
        }

        true
    }

    pub fn consume_wsc(&mut self) {
        self.consume();
        self.skip_ws_and_comments();
    }

    pub fn consume(&mut self) {
        if self.at_eof { return }

        let c = self.peek_char;
        if c == '\n' { self.line_no = self.line_no + 1; }

        if self.chars.len() > 0 {
            self.chars.remove(0);
        }

        if self.chars.len() > 0 {
            self.peek_char = self.chars[0];
        } else {
            self.at_eof = true;
        }
    }

    pub fn skip_ws(&mut self) {
        self.consume_while(|c| c.is_whitespace());
    }

    pub fn skip_ws_and_comments(&mut self) {
        self.skip_ws();
        while let Some(c) = self.peek() {
            if c == '#' {
                self.consume_while(|c| c != '\n');
                if !self.consume_if_eq('\n') {
                    return;
                }
                self.skip_ws();
            } else {
                break;
            }
        }
    }

    pub fn init(&mut self) {
        if self.chars.len() > 0 {
            self.peek_char = self.chars[0];
        } else {
            self.at_eof = true;
        }
    }

    pub fn new(content: &str, file: &str) -> ParseState {
        let mut ps = ParseState {
            chars:     content.chars().collect(),
            peek_char: ' ',
            at_eof:    false,
            line_no:   1,
            file:      String::from(file),
        };
        ps.init();
        ps.skip_ws_and_comments();
        ps
    }
}

//pub fn read_int(it: &mut TE) {
////    let k = it.peek().unwrap();
////    println!("FO: {:?}", k);
//}

pub fn parse_num(ps: &mut ParseState) -> Result<VVal, String> {
    if ps.at_eof { return Err(String::from("EOF, expected num")); }

    let c = ps.peek().unwrap();
    let sign = match c {
        '-' => { ps.consume(); -1 },
        '+' => { ps.consume();  1 },
        _   => 1
    };

    let radix_or_num : String = ps.take_while(|c| c.is_digit(10)).iter().collect();

    let (radix, num) = if ps.consume_if_eq('r') {
        let radix = if let Ok(r) = u8::from_str_radix(&radix_or_num, 10) {
            r
        } else {
            10
        };

        if radix < 2 || radix > 36 {
            return Err(format!("Unsupported radix: {}", radix));
        }

        (radix, ps.take_while(|c| c.is_digit(radix as u32)).iter().collect())
    } else if ps.consume_if_eq('x') {
        if radix_or_num != "0" {
            return Err(format!("Unsupported radix prefix. \
                                Must be '0x'. Found '{}x'", radix_or_num));
        }
        (16, ps.take_while(|c| c.is_digit(16)).iter().collect())
    } else if ps.consume_if_eq('b') {
        if radix_or_num != "0" {
            return Err(format!("Unsupported radix prefix. \
                                Must be '0b'. Found '{}b'", radix_or_num));
        }
        (2, ps.take_while(|c| c.is_digit(2)).iter().collect())
    } else if ps.consume_if_eq('o') {
        if radix_or_num != "0" {
            return Err(format!("Unsupported radix prefix. \
                                Must be '0o'. Found '{}o'", radix_or_num));
        }
        (8, ps.take_while(|c| c.is_digit(8)).iter().collect())
    } else {
        (10, radix_or_num)
    };

    let (is_float, fract_num) = if ps.consume_if_eq('.') {
        let fract_digits : String = ps.take_while(|c| c.is_digit(radix as u32)).iter().collect();
        if let Ok(fract_num) = u64::from_str_radix(&fract_digits, radix as u32) {
            (true, (fract_num as f64) / (radix as f64).powf(fract_digits.len() as f64))
        } else {
            return Err(format!("Invalid fractional digits {}", fract_digits));
        }
    } else {
        (false, 0.0)
    };

    ps.skip_ws_and_comments();

    match u64::from_str_radix(&num, radix as u32) {
        Ok(num) => {
            if is_float {
                if sign == -1 {
                    Ok(VVal::Flt(-((num as f64) + fract_num)))
                } else {
                    Ok(VVal::Flt((num as f64)   + fract_num))
                }
            } else {
                if sign == -1 {
                    Ok(VVal::Int(-(num as i64)))
                } else {
                    Ok(VVal::Int(num as i64))
                }
            }
        },
        _       => Err(String::from("Couldn't parse number")),
    }
}

fn parse_vec(ps: &mut ParseState) -> Result<VVal, String> {
    if !ps.consume_if_eq_wsc('[') { return Err(String::from("Expected '['")); }

    let vec = VVal::vec();
    vec.push(VVal::Syn(Syntax::Lst));

    while ps.peek().unwrap() != ']' {
        let atom = parse_expr(ps)?;
        vec.push(atom);
        if !ps.consume_if_eq_wsc(',') { break; }
    }

    if !ps.consume_if_eq_wsc(']') { return Err(String::from("Expected ']'")); }

    Ok(vec)
}

fn parse_map(ps: &mut ParseState) -> Result<VVal, String> {
    println!("parse_map [{}]", ps.rest());
    if !ps.consume_if_eq_wsc('{') { return Err(String::from("Expected '{'")); }

    let map = VVal::vec();
    map.push(VVal::Syn(Syntax::Map));

    while ps.peek().unwrap() != '}' {
        let key = parse_expr(ps)?;
        if !ps.consume_if_eq_wsc(':') {
            return Err(String::from("Expected ':' after key"));
        }
        let value = parse_expr(ps)?;

        let elem = VVal::vec();
        elem.push(key);
        elem.push(value);
        map.push(elem);

        if !ps.consume_if_eq_wsc(',') { break; }
    }

    if !ps.consume_if_eq_wsc('}') { return Err(String::from("Expected '}'")); }

    Ok(map)
}


fn parse_primitive(ps: &mut ParseState) -> Result<VVal, String> {
    if ps.at_eof { return Err(String::from("EOF, expected num")); }
    let c = ps.peek().unwrap();

    match c {
        '[' => parse_vec(ps),
        '{' => parse_map(ps),
        'n' => {
            if ps.consume_lookahead("none") {
                ps.skip_ws_and_comments();
            } else {
                ps.consume_wsc();
            }
            Ok(VVal::Nul)
        },
        't' => {
            if ps.consume_lookahead("true") {
                ps.skip_ws_and_comments();
            } else {
                ps.consume_wsc();
            }
            Ok(VVal::Bol(true))
        },
        'f' => {
            if ps.consume_lookahead("false") {
                ps.skip_ws_and_comments();
            } else {
                ps.consume_wsc();
            }
            Ok(VVal::Bol(false))
        },
        _   => Ok(VVal::Flt(0.2)),
    }
}

#[allow(dead_code)]
fn is_var(expr: &VVal) -> bool {
    if let Some(ea) = expr.at(0) {
        if let VVal::Syn(s) = ea {
            return s == Syntax::Var;
        }
    }
    return false;
}

fn is_call(expr: &VVal) -> bool {
    if let Some(ea) = expr.at(0) {
        if let VVal::Syn(s) = ea {
            return s == Syntax::Call;
        }
    }
    return false;
}

fn make_to_call(expr: VVal) -> VVal {
    let call = VVal::vec();
    call.push(VVal::Syn(Syntax::Call));
    call.push(expr);
    call
}

fn make_var(identifier: &str) -> VVal {
    let id = VVal::vec();
    id.push(VVal::Syn(Syntax::Var));
    id.push(VVal::Sym(String::from(identifier)));
    return id;
}

fn make_key(identifier: &str) -> VVal {
    let id = VVal::vec();
    id.push(VVal::Syn(Syntax::Key));
    id.push(VVal::Sym(String::from(identifier)));
    return id;
}

fn make_binop(op: &str, left: VVal, right: VVal) -> VVal {
    let call = make_to_call(make_var(op));
    call.push(left);
    call.push(right);
    return call;
}

pub fn parse_identifier(ps: &mut ParseState) -> String {
    let identifier : String =
        ps.take_while_wsc(|c| {
            match c {
               '.' | ',' | ':' | ';' | '{' | '}'
             | '[' | ']' | '(' | ')' | '~' | '|' | '='
                    => false,
                _   => !c.is_whitespace()
            }
        }).iter().collect();
    identifier
}

pub fn parse_value(ps: &mut ParseState) -> Result<VVal, String> {
    println!("parse_value [{}]", ps.rest());
    if let Some(c) = ps.peek() {
        match c {
            '0' ... '9' | '+' | '-' => parse_num(ps),
            '$' => { ps.consume_wsc(); parse_primitive(ps) },
            '[' => {
                ps.consume_wsc();
                let expr = parse_expr(ps)?;
                if !ps.consume_if_eq_wsc(']') {
                    return Err(String::from("Expected ']' in sub expression"));
                }
                Ok(expr)
            },
            '{' => {
                let block = parse_block(ps, true)?;
                block.set_at(0, VVal::Syn(Syntax::Func));
                Ok(block)
            },
            ':' => {
                ps.consume_wsc();
                let id = parse_identifier(ps);
                Ok(make_key(&id))
            },
            _ if c.is_alphanumeric() || c == '_' || c == '@' => {
                let id = parse_identifier(ps);
                Ok(make_var(&id))
            },
            _ => { Err(String::from("FAIL!")) }
        }
    } else {
        Err(String::from("EOF, but expected atom"))
    }
}

pub fn parse_field_access(obj_val: VVal, ps: &mut ParseState) -> Result<VVal, String> {
    let mut obj = obj_val;

    while let Some(c) = ps.peek() {
        if c != '.' { break; }

        ps.consume_wsc();
        let value = parse_value(ps)?;
        if let Some(ea) = value.at(0) {
            if let VVal::Syn(s) = ea {
                if s == Syntax::Var {
                    value.set_at(0, VVal::Syn(Syntax::Key));
                }
            }
        }

        if let Some(c) = ps.peek() {
            match c {
                '=' => {
                    ps.consume_wsc();
                    let field_set = VVal::vec();
                    field_set.push(VVal::Syn(Syntax::SetKey));
                    field_set.push(obj);
                    field_set.push(value);
                    field_set.push(parse_expr(ps)?);
                    return Ok(field_set);
                },
                '(' => {
                    let call = make_to_call(value);
                    call.push(obj);
                    let mut field_call = make_to_call(call);
                    match parse_arg_list(&mut field_call, ps) {
                        Ok(_)    => return Ok(field_call),
                        Err(err) => return Err(err),
                    }
                },
                _ => {
                    let call = make_to_call(value);
                    call.push(obj);
                    obj = call;
                }
            }
        } else {
            let call = make_to_call(value);
            call.push(obj);
            obj = call;
        }
    }

    Ok(obj)
}

pub fn parse_arg_list<'a>(call: &'a mut VVal, ps: &mut ParseState) -> Result<&'a mut VVal, String> {
    if !ps.consume_if_eq_wsc('(') {
        return Err(String::from("Expected '(' while reading call arguments"));
    }

    while let Some(c) = ps.peek() {
        if c == ')' { break; }

        let call_arg = parse_expr(ps)?;
        call.push(call_arg);

        if !ps.consume_if_eq_wsc(',') {
            break;
        }
    }

    if ps.at_eof {
        return Err(String::from("EOF while reading call args"));
    }
    if !ps.consume_if_eq_wsc(')') {
        return Err(String::from("Expected ')' while reading call arguments"));
    }

    return Ok(call);
}

pub fn get_op_prec(op: &str) -> i32 {
    match op {
        "^"                         => 15,
        "*"  | "/" | "%"            => 14,
        "-"  | "+"                  => 13,
        "<<" | ">>"                 => 12,
        "<"  | ">" | "<=" | ">="    => 11,
        "==" | "!="                 => 10,
        "&"                         => 9,
        "&^"                        => 8,
        "&|"                        => 7,
        "&&"                        => 6,
        "||"                        => 5,
        _                           => 0
    }
}

pub fn parse_binop(mut left: VVal, ps: &mut ParseState, op: &str) -> Result<VVal, String> {
    let prec = get_op_prec(op);
    let mut right = parse_call_expr(ps, true, true)?;

    while let Some(next_op) = ps.peek_op() {
        if next_op.len() == 2 {
            ps.consume_wsc();
            ps.consume_wsc();
        } else {
            ps.consume_wsc();
        }

        let next_prec = get_op_prec(&next_op);
        if prec < next_prec {
            right = parse_binop(right, ps, &next_op)?;
        } else {
            left = make_binop(&op, left, right);
            return parse_binop(left, ps, &next_op);
        }
    }

    return Ok(make_binop(op, left, right));
}

pub fn parse_call(mut value: VVal, ps: &mut ParseState, binop_mode: bool) -> Result<VVal, String> {
    println!("parse_call [{}]", ps.rest());
    let mut res_call = VVal::Nul;

    while let Some(c) = ps.peek() {
        println!("PC c={}", c);
        let op = ps.peek_op();
        match c {
            '(' => {
                let mut call = make_to_call(value);
                match parse_arg_list(&mut call, ps) {
                    Ok(_)    => { value = call; },
                    Err(err) => return Err(err),
                }
            },
            '.' => {
                value = parse_field_access(value, ps)?;
            },
            '~' => {
                ps.consume_wsc();
                if let VVal::Nul = res_call { res_call = make_to_call(value); }
                else { res_call.push(value); }
                res_call.push(parse_expr(ps)?);
                // We don't set value here, because it will not be
                // used by '(' or '.' cases anymore!
                // Those will be covered by parse_expr() presumably.
                return Ok(res_call);
            },
            ';' | ')' | ',' | ']' | '|' | '}' | ':' => {
                break;
            },
            _ if op.is_some() => {
                if binop_mode { break; }
                let op = op.unwrap();
                if op.len() == 2 {
                    ps.consume_wsc();
                    ps.consume_wsc();
                } else {
                    ps.consume_wsc();
                }
                value = parse_binop(value, ps, &op)?;
            },
            '=' => {
                return Err(String::from("Unexpected '=' while parsing call"));
            },
            _ => {
                if binop_mode { break; }

                if let VVal::Nul = res_call { res_call = make_to_call(value); }
                else { res_call.push(value); }
                value = parse_value(ps)?;
            },
        }
    }

    if let VVal::Nul = res_call {
        res_call = value;
    } else {
        res_call.push(value);
    }

    Ok(res_call)
}

pub fn parse_expr(ps: &mut ParseState) -> Result<VVal, String> {
    return parse_call_expr(ps, false, false);
}

pub fn parse_call_expr(ps: &mut ParseState, no_pipe: bool, binop_mode: bool) -> Result<VVal, String> {
    println!("parse_expr [{}] np={}", ps.rest(), no_pipe);
    let value = parse_value(ps)?;

    // look ahead, if we see an expression delimiter.
    // because then, this is not going to be a call!
    if ps.lookahead_one_of(";),]}:") {
        return Ok(value);

    } else if ps.at_eof {
        return Ok(value);

    } else if no_pipe && ps.peek().unwrap() == '|' {
        return Ok(value);
    }

    let mut call = value;
    if ps.peek().unwrap() != '|' {
        // if we have something left to read, and it's not a
        // delimiter, then we have to read a call:
        call = parse_call(call, ps, binop_mode)?;
    }

    if ps.at_eof || (no_pipe && ps.peek().unwrap() == '|') {
        return Ok(call);
    }

    while let Some(c) = ps.peek() {
        match c {
            '|' => {
                ps.consume_wsc();
                let mut fn_expr = parse_call_expr(ps, true, binop_mode)?;
                if !is_call(&fn_expr) {
                    fn_expr = make_to_call(fn_expr);
                }
                fn_expr.push(call);
                call = fn_expr;
            },
            _ => {
                break;
            }
        }
    }

    Ok(call)
}

pub fn parse_assignment(ps: &mut ParseState, is_def: bool) -> Result<VVal, String> {
    if ps.at_eof {
        return Err(String::from("EOF but expected assignment"));
    }

    let mut assign = VVal::vec();
    if is_def {
        assign.push(VVal::Syn(Syntax::Def));
    } else {
        assign.push(VVal::Syn(Syntax::Assign));
    }

    if is_def {
        if ps.consume_if_eq_wsc(':') {
            let key = parse_identifier(ps);
            if key == "ref" {
                assign = VVal::vec();
                assign.push(VVal::Syn(Syntax::DefRef));
            }
        }
    }

    let ids = VVal::vec();

    match ps.peek().unwrap() {
        '(' => {
            ps.consume_wsc();

            while let Some(c) = ps.peek() {
                if c == ')' { break; }
                ids.push(VVal::Sym(parse_identifier(ps)));
                if !ps.consume_if_eq_wsc(',') { break; }
            }

            if ps.at_eof {
                return Err(String::from(
                    "EOF while reading destructuring assignment"));
            }

            if !ps.consume_if_eq_wsc(')') {
                return Err(String::from(
                    "Expected ')' at the end of destructuring assignment"));
            }
        },
        _ => { ids.push(VVal::Sym(parse_identifier(ps))); }
    }

    assign.push(ids);

    if !ps.consume_if_eq_wsc('=') {
        return Err(String::from("Expected '=' in assignment"));
    }
    assign.push(parse_expr(ps)?);

    return Ok(assign);
}

pub fn parse_stmt(ps: &mut ParseState) -> Result<VVal, String> {
    println!("parse_stmt [{}]", ps.rest());
    match ps.peek() {
        Some(c) => {
            match c {
                '!' => { ps.consume_wsc(); parse_assignment(ps, true) },
                '.' => { ps.consume_wsc(); parse_assignment(ps, false) },
                '(' => { parse_assignment(ps, false) },
                _   => { parse_expr(ps) },
            }
        },
        None => { Err(String::from("EOF, but expected statement")) }
    }
}

pub fn parse_block(ps: &mut ParseState, is_delimited: bool) -> Result<VVal, String> {
    println!("parse_block [{}]", ps.rest());
    if is_delimited {
        if !ps.consume_if_eq_wsc('{') { return Err(String::from("Expected '{' for block")); }
    }

    let block = VVal::vec();
    block.push(VVal::Syn(Syntax::Block));

    while let Some(c) = ps.peek() {
        if is_delimited { if c == '}' { break; } }

        let next_stmt = parse_stmt(ps)?;
        block.push(next_stmt);

        while ps.consume_if_eq_wsc(';') {
            while ps.consume_if_eq_wsc(';') { }
            if ps.at_eof || ps.consume_if_eq_wsc('}') { return Ok(block); }
            let next_stmt = parse_stmt(ps)?;
            block.push(next_stmt);
        }
    }

    if is_delimited {
        if ps.at_eof { return Err(String::from("EOF while parsing block")); }
        if !ps.consume_if_eq_wsc('}') { return Err(String::from("Expected '}' for block")); }
    }

    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(s: &str) -> ParseState {
        ParseState::new(s, "<testinput>")
    }

    fn parse(s: &str) -> String {
        let mut ps = mk(s);
        match parse_block(&mut ps, false) {
            Ok(v)  => v.s(),
            Err(e) => { panic!(format!("ERROR: {} at '{}' with input '{}'", e, ps.rest(), s)); },
        }
    }

    fn parse_error(s: &str) -> String {
        let mut ps = mk(s);
        match parse_block(&mut ps, false) {
            Ok(v)  => panic!(format!("Expected error but got result: {} for input '{}'",
                                     v.s(), s)),
            Err(e) => { format!("{}", e) },
        }
    }

    #[test]
    fn check_parse_numbers() {
        assert_eq!(parse("#comment \n10;#fom \n"),  "[&Block,10]");
        assert_eq!(parse("10;"),       "[&Block,10]");
        assert_eq!(parse("10.123;"),   "[&Block,10.123]");
        assert_eq!(parse("-10;"),      "[&Block,-10]");
        assert_eq!(parse("-0xFF;"),    "[&Block,-255]");
        assert_eq!(parse("-0xFF.1;"),  "[&Block,-255.0625]");
        assert_eq!(parse("-0xFF.9;"),  "[&Block,-255.5625]");
        assert_eq!(parse("-0xFF.A;"),  "[&Block,-255.625]");
        assert_eq!(parse("-0xFF.F;"),  "[&Block,-255.9375]");
    }

    #[test]
    fn check_parse_vec() {
        assert_eq!(parse("$[10];"),
                   "[&Block,[&Lst,10]]");
        assert_eq!(parse("$[10, 11.23, -30, -0xFF];"),
                   "[&Block,[&Lst,10,11.23,-30,-255]]");
        assert_eq!(parse("$[10, $[1,2,3], 11.23, -30, -0xFF];"),
                   "[&Block,[&Lst,10,[&Lst,1,2,3],11.23,-30,-255]]");
    }

    #[test]
    fn check_calls() {
        assert_eq!(parse("10"),         "[&Block,10]");
        assert_eq!(parse("10;"),        "[&Block,10]");
        assert_eq!(parse("10; 20"),     "[&Block,10,20]");
        assert_eq!(parse("10;;; 20"),   "[&Block,10,20]");
        assert_eq!(parse("10;;; 20;"),  "[&Block,10,20]");
        assert_eq!(parse("10 20;"),     "[&Block,[&Call,10,20]]");
        assert_eq!(parse("[10] 20;"),   "[&Block,[&Call,10,20]]");
    }

    #[test]
    fn check_expr() {
        assert_eq!(parse("10 20 30"),
                   "[&Block,[&Call,10,20,30]]");
        assert_eq!(parse("10 20 30 40"),
                   "[&Block,[&Call,10,20,30,40]]");
        assert_eq!(parse("10 | 20 30"),
                   "[&Block,[&Call,20,30,10]]");
        assert_eq!(parse("10 20 | 30 40"),
                   "[&Block,[&Call,30,40,[&Call,10,20]]]");
        assert_eq!(parse("10 20 | 30 40 | 50"),
                   "[&Block,[&Call,50,[&Call,30,40,[&Call,10,20]]]]");
        assert_eq!(parse("10 | 20 | 30 | 40"),
                   "[&Block,[&Call,40,[&Call,30,[&Call,20,10]]]]");
        assert_eq!(parse("10() | 20 | 30 | 40"),
                   "[&Block,[&Call,40,[&Call,30,[&Call,20,[&Call,10]]]]]");
        assert_eq!(parse("10()() | 20 | 30 | 40"),
                   "[&Block,[&Call,40,[&Call,30,[&Call,20,[&Call,[&Call,10]]]]]]");
        assert_eq!(parse("[10 | 20] | [foo(bar)]"),
                   "[&Block,[&Call,[&Var,$\"foo\"],[&Var,$\"bar\"],[&Call,20,10]]]");
        assert_eq!(parse("10 ~ 20 ~ 30 ~ 40"),
                   "[&Block,[&Call,10,[&Call,20,[&Call,30,40]]]]");
        assert_eq!(parse("10 | 20"),                  "[&Block,[&Call,20,10]]");
        assert_eq!(parse("10 [1 2] [3 4 5] [6 [7]]"), "[&Block,[&Call,10,[&Call,1,2],[&Call,3,4,5],[&Call,6,7]]]");
        assert_eq!(parse("10()"),                     "[&Block,[&Call,10]]");
        assert_eq!(parse("10(20, 30)"),               "[&Block,[&Call,10,20,30]]");
        assert_eq!(parse("10 x(20, 30)"),             "[&Block,[&Call,10,[&Call,[&Var,$\"x\"],20,30]]]");
        assert_eq!(parse("10 x(20, 30) | 50"),        "[&Block,[&Call,50,[&Call,10,[&Call,[&Var,$\"x\"],20,30]]]]");
        assert_eq!(parse("[10].a"),                   "[&Block,[&Call,[&Key,$\"a\"],10]]");
        assert_eq!(parse("a.b"),                      "[&Block,[&Call,[&Key,$\"b\"],[&Var,$\"a\"]]]");
        assert_eq!(parse("10 a.b"),                   "[&Block,[&Call,10,[&Call,[&Key,$\"b\"],[&Var,$\"a\"]]]]");
        assert_eq!(parse("[10].[20]"),                "[&Block,[&Call,20,10]]");
        assert_eq!(parse("10.20 30"),                 "[&Block,[&Call,10.2,30]]");
        assert_eq!(parse("10 20 ~ 30 ~ 40 ~ 50"),     "[&Block,[&Call,10,20,[&Call,30,[&Call,40,50]]]]");
        assert_eq!(parse("10 20 ~ 30 40 ~ 40 1 2 3 ~ 50 60"),  "[&Block,[&Call,10,20,[&Call,30,40,[&Call,40,1,2,3,[&Call,50,60]]]]]");
        assert_eq!(parse("10[10(1,2,3 foo) ~ 4]"),    "[&Block,[&Call,10,[&Call,[&Call,10,1,2,[&Call,3,[&Var,$\"foo\"]]],4]]]");
        assert_eq!(parse("foo.b.c.d"),                "[&Block,[&Call,[&Key,$\"d\"],[&Call,[&Key,$\"c\"],[&Call,[&Key,$\"b\"],[&Var,$\"foo\"]]]]]");
        assert_eq!(parse("foo.b.c.d()"),              "[&Block,[&Call,[&Call,[&Key,$\"d\"],[&Call,[&Key,$\"c\"],[&Call,[&Key,$\"b\"],[&Var,$\"foo\"]]]]]]");
        assert_eq!(parse("foo.b.c.d(1,2,3)"),         "[&Block,[&Call,[&Call,[&Key,$\"d\"],[&Call,[&Key,$\"c\"],[&Call,[&Key,$\"b\"],[&Var,$\"foo\"]]]],1,2,3]]");
        assert_eq!(parse("foo.b.c.d 1 2 3"),          "[&Block,[&Call,[&Call,[&Key,$\"d\"],[&Call,[&Key,$\"c\"],[&Call,[&Key,$\"b\"],[&Var,$\"foo\"]]]],1,2,3]]");
        assert_eq!(parse("[foo.b.c.d] 1 2 3"),        "[&Block,[&Call,[&Call,[&Key,$\"d\"],[&Call,[&Key,$\"c\"],[&Call,[&Key,$\"b\"],[&Var,$\"foo\"]]]],1,2,3]]");
        assert_eq!(parse("foo.a = 10"),               "[&Block,[&SetKey,[&Var,$\"foo\"],[&Key,$\"a\"],10]]");
        assert_eq!(parse("foo.a = 10 | 20"),          "[&Block,[&SetKey,[&Var,$\"foo\"],[&Key,$\"a\"],[&Call,20,10]]]");
        assert_eq!(parse("foo.a = 10 ~ 20"),          "[&Block,[&SetKey,[&Var,$\"foo\"],[&Key,$\"a\"],[&Call,10,20]]]");
        assert_eq!(parse("4 == 5 ~ 10"),              "[&Block,[&Call,[&Var,$\"==\"],4,[&Call,5,10]]]");
    }

    #[test]
    fn check_expr_err() {
        assert_eq!(parse_error("foo.a() = 10"),       "Unexpected \'=\' while parsing call");
    }

    #[test]
    fn check_identifier() {
        assert_eq!(parse("_"),          "[&Block,[&Var,$\"_\"]]");
        assert_eq!(parse("ten"),        "[&Block,[&Var,$\"ten\"]]");
        assert_eq!(parse("tenäß foo"),  "[&Block,[&Call,[&Var,$\"tenäß\"],[&Var,$\"foo\"]]]");
    }

    #[test]
    fn check_primitives() {
        assert_eq!(parse("$n"),         "[&Block,$n]");
        assert_eq!(parse("$none"),      "[&Block,$n]");
        assert_eq!(parse("$t"),         "[&Block,$true]");
        assert_eq!(parse("$true"),      "[&Block,$true]");
        assert_eq!(parse("$f"),         "[&Block,$false]");
        assert_eq!(parse("$false"),     "[&Block,$false]");
    }

    #[test]
    fn check_binops() {
        assert_eq!(parse("20 * 10"),                "[&Block,[&Call,[&Var,$\"*\"],20,10]]");
        assert_eq!(parse("40 20 * 10"),             "[&Block,[&Call,40,[&Call,[&Var,$\"*\"],20,10]]]");
        assert_eq!(parse("40 20 * 10 30"),          "[&Block,[&Call,40,[&Call,[&Var,$\"*\"],20,10],30]]");
        assert_eq!(parse("40 20 * 10()"),           "[&Block,[&Call,40,[&Call,[&Var,$\"*\"],20,[&Call,10]]]]");
        assert_eq!(parse("40 20() * 10()"),         "[&Block,[&Call,40,[&Call,[&Var,$\"*\"],[&Call,20],[&Call,10]]]]");
        assert_eq!(parse("20() * 10()"),            "[&Block,[&Call,[&Var,$\"*\"],[&Call,20],[&Call,10]]]");
        assert_eq!(parse("10 - 20 * 30"),           "[&Block,[&Call,[&Var,$\"-\"],10,[&Call,[&Var,$\"*\"],20,30]]]");
        assert_eq!(parse("10 * 20 - 30"),           "[&Block,[&Call,[&Var,$\"-\"],[&Call,[&Var,$\"*\"],10,20],30]]");
        assert_eq!(parse("10 * 20 - 30 * 2"),       "[&Block,[&Call,[&Var,$\"-\"],[&Call,[&Var,$\"*\"],10,20],[&Call,[&Var,$\"*\"],30,2]]]");
        assert_eq!(parse("10 * 20 * 30"),           "[&Block,[&Call,[&Var,$\"*\"],[&Call,[&Var,$\"*\"],10,20],30]]");
        assert_eq!(parse("10 - 20 - 30 - 40"),      "[&Block,[&Call,[&Var,$\"-\"],[&Call,[&Var,$\"-\"],[&Call,[&Var,$\"-\"],10,20],30],40]]");
        assert_eq!(parse("10 - 20 - [30 - 40]"),    "[&Block,[&Call,[&Var,$\"-\"],[&Call,[&Var,$\"-\"],10,20],[&Call,[&Var,$\"-\"],30,40]]]");
    }

    #[test]
    fn check_assignments() {
        assert_eq!(parse("!x=10;"),              "[&Block,[&Def,[$\"x\"],10]]");
        assert_eq!(parse("! x = 10 ;"),          "[&Block,[&Def,[$\"x\"],10]]");
        assert_eq!(parse("! x = 10"),            "[&Block,[&Def,[$\"x\"],10]]");
        assert_eq!(parse("!:ref x = 10"),        "[&Block,[&DefRef,[$\"x\"],10]]");
        assert_eq!(parse("!:ref (a,b) = 10"),    "[&Block,[&DefRef,[$\"a\",$\"b\"],10]]");
        assert_eq!(parse(". (a,b) = 10"),        "[&Block,[&Assign,[$\"a\",$\"b\"],10]]");
        assert_eq!(parse("(a,b)=10"),            "[&Block,[&Assign,[$\"a\",$\"b\"],10]]");
    }

    #[test]
    fn check_func() {
        assert_eq!(parse("{}"),           "[&Block,[&Func]]");
        assert_eq!(parse("{10;}"),        "[&Block,[&Func,10]]");
        assert_eq!(parse("{10;;;}"),      "[&Block,[&Func,10]]");
        assert_eq!(parse("{10; 20}"),     "[&Block,[&Func,10,20]]");
        assert_eq!(parse("{ 10 } { }"),   "[&Block,[&Call,[&Func,10],[&Func]]]");
    }

    #[test]
    fn check_map() {
        assert_eq!(parse("${a:10}"),   "[&Block,[&Map,[[&Var,$\"a\"],10]]]");
        assert_eq!(parse("${:a:10}"),  "[&Block,[&Map,[[&Key,$\"a\"],10]]]");
    }
}
