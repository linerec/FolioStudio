use folio_model::{Affine, PathPoint, Point, SubPath};

pub fn parse(d: &str, t: &Affine) -> Option<Vec<SubPath>> {
    if d.trim_start().chars().next().is_some_and(|c|!matches!(c,'M'|'m')) {return None;}
    let b: Vec<char> = d.chars().collect();
    let mut i = 0;
    let mut out: Vec<SubPath> = vec![];
    let mut cur = SubPath::default();
    let (mut pen, mut start) = (Point::new(0.0, 0.0), Point::new(0.0, 0.0));
    // the previous curve's last control point (for S and T), and which kind it was
    let mut last_ctl: Option<(Point, bool)> = None;
    let mut cmd = ' ';
    let map = |p: Point| t.apply(p);
    fn number(b: &[char], i: &mut usize) -> Option<f64> {
        while *i < b.len() && (b[*i].is_whitespace() || b[*i] == ',') {
            *i += 1;
        }
        let from = *i;
        if *i < b.len() && (b[*i] == '-' || b[*i] == '+') {
            *i += 1;
        }
        let mut dot = false;
        while *i < b.len() && (b[*i].is_ascii_digit() || (b[*i] == '.' && !dot)) {
            dot |= b[*i] == '.';
            *i += 1;
        }
        if *i < b.len() && (b[*i] == 'e' || b[*i] == 'E') {
            let mut j = *i + 1;
            if j < b.len() && (b[j] == '-' || b[j] == '+') {
                j += 1;
            }
            if j < b.len() && b[j].is_ascii_digit() {
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                *i = j;
            }
        }
        b[from..*i].iter().collect::<String>().parse::<f64>().ok().filter(|v| v.is_finite())
    }
    let skip = |i: &mut usize| {
        while *i < b.len() && (b[*i].is_whitespace() || b[*i] == ',') {
            *i += 1;
        }
    };
    loop {
        skip(&mut i);
        if i >= b.len() {
            break;
        }
        if b[i].is_ascii_alphabetic() {
            cmd = b[i];
            i += 1;
            if cmd == 'Z' || cmd == 'z' {
                if !cur.points.is_empty() {
                    cur.open = false;
                    out.push(std::mem::take(&mut cur));
                }
                pen = start;
                last_ctl = None;
                continue;
            }
        } else if cmd == ' ' || cmd == 'Z' || cmd == 'z' {
            return None;
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { pen } else { Point::new(0.0, 0.0) };
        let n = |i: &mut usize| number(&b, i);
        let line = |cur: &mut SubPath, to: Point| {
            let a = map(to);
            cur.points.push(PathPoint { anchor: a, left: a, right: a });
        };
        let curve = |cur: &mut SubPath, c1: Point, c2: Point, to: Point| {
            if let Some(last) = cur.points.last_mut() {
                last.right = map(c1);
            }
            let a = map(to);
            cur.points.push(PathPoint { anchor: a, left: map(c2), right: a });
        };
        if cur.points.is_empty() && !cmd.eq_ignore_ascii_case(&'M') {
            line(&mut cur, pen); // a segment right after z continues from the subpath start
        }
        match cmd.to_ascii_uppercase() {
            'M' => {
                let to = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                if !cur.points.is_empty() {
                    cur.open = true;
                    out.push(std::mem::take(&mut cur));
                }
                line(&mut cur, to);
                pen = to;
                start = to;
                last_ctl = None;
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let to = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                line(&mut cur, to);
                pen = to;
                last_ctl = None;
            }
            'H' => {
                let to = Point::new(base.x + n(&mut i)?, pen.y);
                line(&mut cur, to);
                pen = to;
                last_ctl = None;
            }
            'V' => {
                let to = Point::new(pen.x, base.y + n(&mut i)?);
                line(&mut cur, to);
                pen = to;
                last_ctl = None;
            }
            'C' => {
                let c1 = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                let c2 = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                let to = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                curve(&mut cur, c1, c2, to);
                pen = to;
                last_ctl = Some((c2, true));
            }
            'S' => {
                let c1 = match last_ctl {
                    Some((c, true)) => Point::new(2.0 * pen.x - c.x, 2.0 * pen.y - c.y),
                    _ => pen,
                };
                let c2 = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                let to = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                curve(&mut cur, c1, c2, to);
                pen = to;
                last_ctl = Some((c2, true));
            }
            'Q' | 'T' => {
                let q = if cmd.eq_ignore_ascii_case(&'Q') {
                    Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?)
                } else {
                    match last_ctl {
                        Some((c, false)) => Point::new(2.0 * pen.x - c.x, 2.0 * pen.y - c.y),
                        _ => pen,
                    }
                };
                let to = Point::new(base.x + n(&mut i)?, base.y + n(&mut i)?);
                // a quadratic as the cubic with the same curve
                let third = |from: Point, ctl: Point| Point::new(from.x + 2.0 / 3.0 * (ctl.x - from.x), from.y + 2.0 / 3.0 * (ctl.y - from.y));
                curve(&mut cur, third(pen, q), third(to, q), to);
                pen = to;
                last_ctl = Some((q, false));
            }
            'A' => {
                let (rx, ry, angle) = (n(&mut i)?, n(&mut i)?, n(&mut i)?);
                skip(&mut i);
                let large = match b.get(i)? { '0' => false, '1' => true, _ => return None }; i += 1;
                skip(&mut i);
                let sweep = match b.get(i)? { '0' => false, '1' => true, _ => return None }; i += 1;
                let to = Point::new(base.x+n(&mut i)?, base.y+n(&mut i)?);
                if rx == 0. || ry == 0. { line(&mut cur, to); }
                else { for (c1,c2,end) in arc(pen,to,rx,ry,angle,large,sweep)? { curve(&mut cur,c1,c2,end); } }
                pen = to; last_ctl = None;
            }
            _ => return None,
        }
    }
    if !cur.points.is_empty() {
        cur.open = true;
        out.push(cur);
    }
    out.iter().flat_map(|p| &p.points).all(|p| [p.anchor, p.left, p.right].iter().all(|p| p.x.is_finite() && p.y.is_finite())).then_some(out)
}

fn arc(from: Point, to: Point, rx: f64, ry: f64, angle: f64, large: bool, sweep: bool) -> Option<Vec<(Point,Point,Point)>> {
    if from == to { return Some(Vec::new()); }
    let (mut rx,mut ry) = (rx.abs(),ry.abs());
    let (sin,cos) = angle.to_radians().sin_cos();
    let (dx,dy) = ((from.x-to.x)/2.,(from.y-to.y)/2.);
    let (x,y) = (cos*dx+sin*dy,-sin*dx+cos*dy);
    let scale = ((x/rx).powi(2)+(y/ry).powi(2)).sqrt().max(1.);
    rx *= scale; ry *= scale;
    let (xx,yy) = (x/rx,y/ry);
    let k = ((1.-xx*xx-yy*yy)/(xx*xx+yy*yy)).max(0.).sqrt() * if large==sweep {-1.} else {1.};
    let (cx,cy) = (k*rx*yy,-k*ry*xx);
    let center = Point::new(cos*cx-sin*cy+(from.x+to.x)/2.,sin*cx+cos*cy+(from.y+to.y)/2.);
    let a = ((y-cy)/ry).atan2((x-cx)/rx);
    let end = ((-y-cy)/ry).atan2((-x-cx)/rx);
    let mut delta = end-a;
    if sweep && delta<0. {delta+=std::f64::consts::TAU;}
    if !sweep && delta>0. {delta-=std::f64::consts::TAU;}
    if ![rx,ry,center.x,center.y,a,delta].iter().all(|v|v.is_finite()) { return None; }
    let pieces = (delta.abs()/std::f64::consts::FRAC_PI_2).ceil().max(1.) as usize;
    let step = delta/pieces as f64;
    let p = |a:f64| Point::new(center.x+cos*rx*a.cos()-sin*ry*a.sin(),center.y+sin*rx*a.cos()+cos*ry*a.sin());
    let d = |a:f64| Point::new(-cos*rx*a.sin()-sin*ry*a.cos(),-sin*rx*a.sin()+cos*ry*a.cos());
    let mut curves=Vec::new();
    for i in 0..pieces {
        let (a,b)=(a+i as f64*step,a+(i+1) as f64*step);
        let k=4./3.*(step/4.).tan();
        let (p0,p1,d0,d1)=(p(a),p(b),d(a),d(b));
        curves.push((Point::new(p0.x+k*d0.x,p0.y+k*d0.y),Point::new(p1.x-k*d1.x,p1.y-k*d1.y),if i+1==pieces {to} else {p1}));
    }
    Some(curves)
}
