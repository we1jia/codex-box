// src-tauri/src/config/diff.rs
use crate::config::model::{DiffKind, DiffLine};

pub fn between(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();

    let lcs = compute_lcs(&old_lines, &new_lines);
    let mut result = Vec::new();
    let mut o = 0;
    let mut n = 0;
    let mut lcs_idx = 0;

    while o < old_lines.len() || n < new_lines.len() {
        if lcs_idx < lcs.len() {
            let (li, lj) = lcs[lcs_idx];
            while o < li {
                result.push(DiffLine {
                    kind: DiffKind::Delete,
                    content: old_lines[o].to_string(),
                    old_line: Some(o + 1),
                    new_line: None,
                });
                o += 1;
            }
            while n < lj {
                result.push(DiffLine {
                    kind: DiffKind::Insert,
                    content: new_lines[n].to_string(),
                    old_line: None,
                    new_line: Some(n + 1),
                });
                n += 1;
            }
            result.push(DiffLine {
                kind: DiffKind::Context,
                content: old_lines[o].to_string(),
                old_line: Some(o + 1),
                new_line: Some(n + 1),
            });
            o += 1;
            n += 1;
            lcs_idx += 1;
        } else {
            while o < old_lines.len() {
                result.push(DiffLine {
                    kind: DiffKind::Delete,
                    content: old_lines[o].to_string(),
                    old_line: Some(o + 1),
                    new_line: None,
                });
                o += 1;
            }
            while n < new_lines.len() {
                result.push(DiffLine {
                    kind: DiffKind::Insert,
                    content: new_lines[n].to_string(),
                    old_line: None,
                    new_line: Some(n + 1),
                });
                n += 1;
            }
        }
    }

    result
}

fn compute_lcs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..m {
        for j in 0..n {
            if a[i] == b[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }
    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    result.reverse();
    result
}

/// 统计各类型行数
pub fn count_by_kind(lines: &[DiffLine]) -> (usize, usize, usize) {
    let mut ctx = 0;
    let mut ins = 0;
    let mut del = 0;
    for l in lines {
        match l.kind {
            DiffKind::Context => ctx += 1,
            DiffKind::Insert => ins += 1,
            DiffKind::Delete => del += 1,
        }
    }
    (ctx, ins, del)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_produce_only_context() {
        let lines = between("a\nb\n", "a\nb\n");
        let (ctx, ins, del) = count_by_kind(&lines);
        assert_eq!(ctx, 2);
        assert_eq!(ins, 0);
        assert_eq!(del, 0);
    }

    #[test]
    fn added_line_marked_insert() {
        let lines = between("a\n", "a\nb\n");
        let has_insert = lines.iter().any(|l| l.kind == DiffKind::Insert);
        assert!(has_insert);
    }

    #[test]
    fn removed_line_marked_delete() {
        let lines = between("a\nb\n", "a\n");
        let has_delete = lines.iter().any(|l| l.kind == DiffKind::Delete);
        assert!(has_delete);
    }

    #[test]
    fn empty_inputs_produce_empty_diff() {
        let lines = between("", "");
        assert!(lines.is_empty());
    }
}
