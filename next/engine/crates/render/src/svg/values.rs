use folio_model::Affine;

pub fn numbers(s: &str) -> Option<Vec<f64>> {
    let mut rest = s.trim();
    let mut out = Vec::new();
    while !rest.is_empty() {
        let b = rest.as_bytes();
        let mut i = usize::from(matches!(b[0], b'+' | b'-'));
        let mut digits = 0;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; digits += 1; }
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() { i += 1; digits += 1; }
        }
        if digits == 0 { return None; }
        if i < b.len() && matches!(b[i], b'e' | b'E') {
            i += 1;
            if i < b.len() && matches!(b[i], b'+' | b'-') { i += 1; }
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
            if i == start { return None; }
        }
        out.push(rest[..i].parse::<f64>().ok().filter(|v| v.is_finite())?);
        rest = rest[i..].trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    }
    Some(out)
}

pub fn length(s: &str, reference: f64) -> Option<f64> {
    let s = s.trim();
    let (s, scale) = if let Some(v) = s.strip_suffix('%') { (v, reference / 100.0) }
        else if let Some(v) = s.strip_suffix("px") { (v, 1.0) }
        else if let Some(v) = s.strip_suffix("pt") { (v, 96.0 / 72.0) }
        else if let Some(v) = s.strip_suffix("mm") { (v, 96.0 / 25.4) }
        else if let Some(v) = s.strip_suffix("cm") { (v, 96.0 / 2.54) }
        else if let Some(v) = s.strip_suffix("in") { (v, 96.0) }
        else if let Some(v) = s.strip_suffix("pc") { (v, 16.0) }
        else { (s, 1.0) };
    s.parse::<f64>().ok().map(|v| v * scale).filter(|v| v.is_finite())
}

pub fn transform(list: &str) -> Option<Affine> {
    let mut out = Affine::IDENTITY;
    let mut rest = list.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        if close <= open { return None; }
        let args = numbers(&rest[open+1..close])?;
        let one = match (rest[..open].trim(), args.as_slice()) {
            ("translate", [x]) => Affine::translate(*x, 0.),
            ("translate", [x,y]) => Affine::translate(*x,*y),
            ("scale", [s]) => Affine::scale(*s,*s),
            ("scale", [x,y]) => Affine::scale(*x,*y),
            ("matrix", [a,b,c,d,e,f]) => Affine::new(*a,*b,*c,*d,*e,*f),
            ("rotate", [a]) => rotation(*a),
            ("rotate", [a,x,y]) => Affine::translate(*x,*y).then_inner(&rotation(*a)).then_inner(&Affine::translate(-x,-y)),
            ("skewX", [a]) => Affine::new(1.,0.,a.to_radians().tan(),1.,0.,0.),
            ("skewY", [a]) => Affine::new(1.,a.to_radians().tan(),0.,1.,0.,0.),
            _ => return None,
        };
        out = out.then_inner(&one);
        rest = rest[close+1..].trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    }
    [out.a,out.b,out.c,out.d,out.tx,out.ty].iter().all(|v| v.is_finite()).then_some(out)
}

fn rotation(a: f64) -> Affine {
    let (s,c) = a.to_radians().sin_cos();
    Affine::new(c,s,-s,c,0.,0.)
}

pub fn color(value: &str) -> Option<Option<[u8; 3]>> {
    let lower = value.trim().to_ascii_lowercase();
    let v = lower.as_str();
    if v == "none" { return Some(None); }
    if let Some(hex) = v.strip_prefix('#') {
        let digit = |i: usize, n: usize| u8::from_str_radix(hex.get(i..i+n)?,16).ok();
        return Some(Some(match hex.len() {
            6 => [digit(0,2)?,digit(2,2)?,digit(4,2)?],
            3 => [digit(0,1)?*17,digit(1,1)?*17,digit(2,1)?*17],
            _ => return None,
        }));
    }
    if let Some(args) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let p: Vec<_> = args.split(|c:char| c==',' || c.is_whitespace()).filter(|s| !s.is_empty()).collect();
        if p.len()!=3 { return None; }
        let c = |s:&str| length(s,255.).map(|v| v.round().clamp(0.,255.) as u8);
        return Some(Some([c(p[0])?,c(p[1])?,c(p[2])?]));
    }
    Some(Some(match v {
        "black" => [0,0,0], "white" => [255,255,255], "red" => [255,0,0], "green" => [0,128,0], "blue" => [0,0,255],
        "yellow" => [255,255,0], "cyan"|"aqua" => [0,255,255], "magenta"|"fuchsia" => [255,0,255],
        "gray"|"grey" => [128,128,128], "silver" => [192,192,192], "maroon" => [128,0,0], "olive" => [128,128,0],
        "lime" => [0,255,0], "teal" => [0,128,128], "navy" => [0,0,128], "purple" => [128,0,128], "orange" => [255,165,0],
        _ => return None,
    }))
}
