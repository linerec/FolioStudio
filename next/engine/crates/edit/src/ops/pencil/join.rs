use super::bezier::*;
use super::chain::{Chain, merge};
use super::fit::fit;

fn fit_bridge(left: Chain, right: Chain) -> Vec<Point> {
    let a = left.last();
    let b = right.first();
    let h = (b - a).length() / 2.;
    let bridge = Chain::new(
        vec![
            a,
            a + left.tangent_end().unit() * h,
            b - right.tangent_start().unit() * h,
            b,
        ],
        left.tol,
    );
    let samples = left
        .sample_points()
        .chain(bridge.sample_points())
        .chain(right.sample_points())
        .collect::<Vec<_>>();
    fit(&samples, left.tol, left.tangent_start(), right.tangent_end(), 72.)
}

fn cut_prefix(prefix: &Chain, position: f64, original_end: f64) -> (Chain, Chain) {
    prefix.cut(ratio(position, original_end) * prefix.len())
}

pub(super) fn join(left: Chain, right: Chain) -> Chain {
    if left.len() == 0. {
        return right;
    }
    if right.len() == 0. {
        return left;
    }
    let tol = left.tol;
    let width = (3. * (left.last() - right.first()).length()).max(tol);
    let left_end = (left.len() - width).max(0.);
    let left_start = left.anchor_length((left_end - width).max(0.), false);
    let right_start = width.min(right.len());
    let right_end = right.anchor_length((width + right_start).min(right.len()), true);
    let (prefix, _) = left.cut(left_end);
    let (head, left_window) = cut_prefix(&prefix, left_start, left_end);
    let (prefix, tail) = right.cut(right_end);
    let (_, right_window) = cut_prefix(&prefix, right_start, right_end);
    let bridge = fit_bridge(left_window, right_window);
    Chain::new(merge(merge(head.controls, &bridge), &tail.controls), tol)
}

pub(super) fn close_loop(mut curve: Chain) -> Chain {
    let tol = curve.tol;
    let length = curve.len();
    let width = (3. * (curve.last() - curve.first()).length()).max(tol);
    let mut left_end = (length - width).max(0.);
    let mut right_start = width.min(length);
    if left_end < right_start {
        left_end = (left_end + right_start) / 2.;
        right_start = left_end;
    }
    let mut left_start = curve.anchor_length((left_end - width).max(0.), false);
    let mut right_end = curve.anchor_length((right_start + width).min(length), true);
    if left_start < right_end {
        let mid = (left_end + right_start) / 2.;
        let (head, tail) = curve.cut(mid);
        left_end = ratio(left_end, mid) * head.len();
        right_start = ratio(right_start - mid, length - mid) * tail.len() + head.len();
        curve = Chain::new(merge(head.controls, &tail.controls), tol);
        left_start = curve.anchor_length(left_end, false);
        right_end = curve.anchor_length(right_start, true);
    }
    let (prefix, _) = curve.cut(left_end);
    let (prefix, left) = cut_prefix(&prefix, left_start, left_end);
    let (prefix, middle) = cut_prefix(&prefix, right_end, left_start);
    let (_, right) = cut_prefix(&prefix, right_start, right_end);
    Chain::new(merge(middle.controls, &fit_bridge(left, right)), tol)
}
