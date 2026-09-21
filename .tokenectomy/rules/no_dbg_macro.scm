;; @id: CORP_SEC_001
;; @name: no-dbg-macro
;; @severity: error
;; @message: dbg!(...) macro found. Debug macros must not be committed to production.
;; @fix_hint: Remove dbg! or use structured tracing::debug!
;; @languages: rust

(macro_invocation
  macro: (identifier) @name (#eq? @name "dbg")) @match
