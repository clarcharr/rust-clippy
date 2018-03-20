use rustc::lint::*;
use rustc::hir::*;
use syntax::ast::NodeId;
use utils::{in_macro, match_def_path, match_trait_method, same_tys, snippet, span_lint_and_then};
use utils::{opt_def_id, paths, resolve_node};

/// **What it does:** Checks for always-identical `Into`/`From`/`AsRef`/`AsMut`/etc. conversions.
///
/// **Why is this bad?** Redundant code.
///
/// **Known problems:** None.
///
/// **Example:**
/// ```rust
/// // format!() returns a `String`
/// let s: String = format!("hello").into();
/// ```
declare_lint! {
    pub IDENTITY_CONVERSION,
    Warn,
    "using always-identical `Into`/`From`/`AsRef`/`AsMut`/etc. conversions"
}

#[derive(Default)]
pub struct IdentityConversion {
    try_desugar_arm: Vec<NodeId>,
}

impl LintPass for IdentityConversion {
    fn get_lints(&self) -> LintArray {
        lint_array!(IDENTITY_CONVERSION)
    }
}

const REDUNDANT_METHOD: &[(&[&str], &str)] = &[
    (&paths::INTO, "into"),
    (&paths::ASREF_TRAIT, "as_ref"),
    (&paths::ASMUT_TRAIT, "as_mut"),
    (&paths::BORROW_TRAIT, "borrow"),
    (&paths::BORROW_MUT_TRAIT, "borrow_mut"),
    (&paths::ITERATOR, "by_ref"),
];

const REDUNDANT_STATIC_METHOD: &[(&[&str], &str)] = &[
    (&paths::FROM_TRAIT, "from"),
];

impl<'a, 'tcx> LateLintPass<'a, 'tcx> for IdentityConversion {
    fn check_expr(&mut self, cx: &LateContext<'a, 'tcx>, e: &'tcx Expr) {
        if in_macro(e.span) {
            return;
        }

        if Some(&e.id) == self.try_desugar_arm.last() {
            return;
        }

        match e.node {
            ExprMatch(_, ref arms, MatchSource::TryDesugar) => {
                let e = match arms[0].body.node {
                    ExprRet(Some(ref e)) | ExprBreak(_, Some(ref e)) => e,
                    _ => return,
                };
                if let ExprCall(_, ref args) = e.node {
                    self.try_desugar_arm.push(args[0].id);
                } else {
                    return;
                }
            },

            ExprMethodCall(ref name, .., ref args) => {
                for &(trait_, method) in REDUNDANT_METHOD {
                    if match_trait_method(cx, e, &trait_) && &*name.name.as_str() == method {
                        let a = cx.tables.expr_ty(e);
                        let b = cx.tables.expr_ty(&args[0]);
                        if same_tys(cx, a, b) {
                            let sugg = snippet(cx, args[0].span, "<expr>").into_owned();
                            span_lint_and_then(cx, IDENTITY_CONVERSION, e.span, "identical conversion", |db| {
                                db.span_suggestion(
                                    e.span,
                                    &format!("consider removing `.{}()`", method),
                                    sugg,
                                );
                            });
                        }
                    }
                }
            },

            ExprCall(ref path, ref args) => if let ExprPath(ref qpath) = path.node {
                if let Some(def_id) = opt_def_id(resolve_node(cx, qpath, path.hir_id)) {
                    for &(trait_, method) in REDUNDANT_STATIC_METHOD.iter().chain(REDUNDANT_METHOD) {
                        if match_def_path(cx.tcx, def_id, &trait_) {
                            let method_name = match *qpath {
                                QPath::Resolved(_, ref path) => {
                                    path.segments.last().unwrap().name
                                },
                                QPath::TypeRelative(_, ref seg) => {
                                    seg.name
                                },
                            };
                            if method_name.as_str() == method {
                                let a = cx.tables.expr_ty(e);
                                let b = cx.tables.expr_ty(&args[0]);
                                if same_tys(cx, a, b) {
                                    let sugg = snippet(cx, args[0].span, "<expr>").into_owned();
                                    let sugg_msg = format!(
                                        "consider removing `{}()`",
                                        snippet(
                                            cx, path.span,
                                            &format!("{}::{}", trait_.last().unwrap(), method),
                                        )
                                    );
                                    span_lint_and_then(cx, IDENTITY_CONVERSION, e.span, "identical conversion", |db| {
                                        db.span_suggestion(e.span, &sugg_msg, sugg);
                                    });
                                }
                            }
                        }
                    }
                }
            },

            _ => {},
        }
    }

    fn check_expr_post(&mut self, _: &LateContext<'a, 'tcx>, e: &'tcx Expr) {
        if Some(&e.id) == self.try_desugar_arm.last() {
            self.try_desugar_arm.pop();
        }
    }
}
