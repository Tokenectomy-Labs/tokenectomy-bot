;; @id: TB_CUSTOM_001
;; @name: no-hardcoded-secret
;; @severity: error
;; @message: Potential hardcoded secret or token assignment detected in source code
;; @fix_hint: Move credentials to secure environment variables or secret vaults
;; @languages: typescript, javascript

(variable_declarator
  name: (identifier) @id (#match? @id "(?i)(api_?key|secret|token|password)")
  value: (string) @val) @match
