;;; birnam-mode.el --- Major mode for the Birnam language -*- lexical-binding: t; -*-

;; Author: Birnam contributors
;; Keywords: languages, birnam
;; Version: 0.2.0
;; Package-Requires: ((emacs "26.1"))

;;; Commentary:

;; A basic major mode for editing Birnam source (`.birnam') files, plus an
;; inferior REPL via comint (`run-birnam').
;;
;; Birnam is a prototypal, Smalltalk-family language: there are no classes.
;; Behaviour lives on objects/prototypes, code is bracketed message sends
;; (`[recv selector: arg]'), and a historied fact store backs identity.  Every
;; well-known name is a global binding on a prototype (`System', `Object',
;; `Vector', `Reflect', …).  The mode highlights:
;;
;;   //  line comments
;;   ".." string literals (with `\\' escapes)
;;   bracket forms      [ ]  block / send,  [:x :y | ..]  block args,  [| ..]
;;   literal collections  #[ e e e ]  vector,  { k -> v, .. }  map
;;   keyword selectors  at:put:  inject:into:
;;   symbols            #foo  #at:put:  #+  (also operator symbols)
;;   query vars         ?name   (datalog surface)
;;   globals            System  Object  Vector  …  (Capitalized identifiers)
;;   reserved/literals  let  nil  true  false  self
;;   assignment :=  map arrow ->  pipe |>  statement separator ;
;;
;; Install:
;;   (add-to-list 'load-path "/path/to/birnam/tools/emacs")
;;   (require 'birnam-mode)
;;
;; Set the binary if `birnam' is not on PATH:
;;   (setq birnam-program "/path/to/zig-out/bin/birnam")

;;; Code:

(require 'comint)

(defgroup birnam nil
  "Major mode for editing Birnam source."
  :group 'languages
  :prefix "birnam-")

(defcustom birnam-indent-offset 2
  "Number of columns to indent per open bracket."
  :type 'integer
  :group 'birnam)

(defcustom birnam-program "birnam"
  "Path to the Birnam executable used for the REPL and script runs."
  :type 'string
  :group 'birnam)

(defcustom birnam-program-args '("--repl")
  "Arguments passed to `birnam-program' when starting the REPL.
Defaults to `--repl', which starts the interactive read-eval-print loop."
  :type '(repeat string)
  :group 'birnam)

;;;; Faces / font-lock ---------------------------------------------------

(defconst birnam--keywords
  '("let" "nil" "true" "false" "self")
  "Reserved words and built-in literals.
`let' opens a binding; `nil'/`true'/`false' are literals lowered by
codegen; `self' is the bound method receiver.  There is no `super'.")

(defconst birnam-font-lock-keywords
  (let ((kw (regexp-opt birnam--keywords 'symbols)))
    `(;; reserved words / literals
      (,kw . font-lock-keyword-face)
      ;; operators: assignment `:=', map-entry arrow `->', thread pipe `|>'
      ("\\(:=\\|->\\||>\\)" . font-lock-builtin-face)
      ;; symbols: #foo, #at:put:, and operator symbols #+ #>= #,
      ;; (the literal openers #[ #{ #( never reach here — they are not #ident).
      ("#\\(?:[A-Za-z_][A-Za-z0-9_]*\\(?::[A-Za-z0-9_]*\\)*\\|[-=<>&|,+*/~%@!^]+\\)"
       . font-lock-constant-face)
      ;; datalog query variables: ?name
      ("\\?[A-Za-z_][A-Za-z0-9_]*" . font-lock-variable-name-face)
      ;; well-known globals / prototypes: any Capitalized identifier is a
      ;; global binding on a prototype (System, Object, Vector, Reflect, …).
      ("\\_<[A-Z][A-Za-z0-9_]*\\_>" . font-lock-type-face)
      ;; keyword selector parts: foo:  (identifier immediately followed by ':')
      ("\\_<[A-Za-z_][A-Za-z0-9_]*:" . font-lock-function-name-face)
      ;; block parameters / temporaries: :x  (leading colon)
      (":[A-Za-z_][A-Za-z0-9_]*\\_>" . font-lock-variable-name-face)
      ;; numbers (int / float / negative)
      ("\\_<-?[0-9]+\\(?:\\.[0-9]+\\)?\\_>" . font-lock-constant-face)))
  "Font-lock highlighting for `birnam-mode'.")

;;;; Syntax table --------------------------------------------------------

(defvar birnam-mode-syntax-table
  (let ((st (make-syntax-table)))
    ;; `//' line comments.  `/' is also division: mark it as the first AND
    ;; second char of a two-char comment opener, so a lone `/' stays an
    ;; operator and only `//' starts a comment.
    (modify-syntax-entry ?/ ". 12" st)
    (modify-syntax-entry ?\n ">" st)
    ;; strings, with `\\' as the escape char (birnam's string rule treats a
    ;; backslash as escaping the next byte, so `\\"' is a literal quote).
    (modify-syntax-entry ?\" "\"" st)
    (modify-syntax-entry ?\\ "\\" st)
    ;; bracket pairs
    (modify-syntax-entry ?\[ "(]" st)
    (modify-syntax-entry ?\] ")[" st)
    (modify-syntax-entry ?{ "(}" st)
    (modify-syntax-entry ?} "){" st)
    (modify-syntax-entry ?\( "()" st)
    (modify-syntax-entry ?\) ")(" st)
    ;; symbol-constituents used in identifiers/selectors/symbols
    (modify-syntax-entry ?_ "_" st)
    (modify-syntax-entry ?# "'" st)
    (modify-syntax-entry ?? "'" st)
    ;; operators
    (modify-syntax-entry ?: "." st)
    (modify-syntax-entry ?= "." st)
    (modify-syntax-entry ?| "." st)
    (modify-syntax-entry ?+ "." st)
    (modify-syntax-entry ?- "." st)
    (modify-syntax-entry ?* "." st)
    (modify-syntax-entry ?< "." st)
    (modify-syntax-entry ?> "." st)
    (modify-syntax-entry ?\; "." st)
    st)
  "Syntax table for `birnam-mode'.")

;;;; Indentation ---------------------------------------------------------

(defun birnam-indent-line ()
  "Indent the current line for `birnam-mode'.

Indentation is driven by bracket nesting depth (`[' `{' `('): each
open bracket adds `birnam-indent-offset' columns.  A line that begins
by closing brackets is dedented to match the depth after those
closers.  This is deliberately simple — re-indent (TAB) and adjust by
hand where a send wraps mid-expression."
  (let (indent)
    (save-excursion
      (beginning-of-line)
      (let* ((ppss (syntax-ppss))
             (depth (car ppss))
             ;; in a string or comment? leave the line alone.
             (in-str-or-cmt (or (nth 3 ppss) (nth 4 ppss))))
        (unless in-str-or-cmt
          ;; if the line opens with closing bracket(s), they belong to the
          ;; enclosing (shallower) level.
          (when (looking-at "[ \t]*\\s)")
            (setq depth (max 0 (1- depth))))
          (setq indent (* depth birnam-indent-offset)))))
    (when indent
      (if (<= (current-column) (current-indentation))
          (indent-line-to indent)
        (save-excursion (indent-line-to indent))))))

;;;; REPL (comint) -------------------------------------------------------

(defvar birnam-repl-buffer-name "*birnam*"
  "Buffer name for the inferior Birnam REPL.")

(defun birnam--repl-process ()
  "Return the live Birnam REPL process, or nil."
  (get-buffer-process birnam-repl-buffer-name))

;;;###autoload
(defun run-birnam ()
  "Run an inferior Birnam REPL in `birnam-repl-buffer-name'.
Starts `birnam-program' with `birnam-program-args' (`--repl' by
default).  The REPL reads Birnam directly: an expression prints its
value, `let x := …' binds a name that persists across lines, and
multi-line forms continue until the brackets balance.  Commands are
`:help', `:globals', and `:quit'."
  (interactive)
  (unless (comint-check-proc birnam-repl-buffer-name)
    (apply #'make-comint-in-buffer "birnam" birnam-repl-buffer-name
           birnam-program nil birnam-program-args))
  (pop-to-buffer birnam-repl-buffer-name))

(defun birnam--ensure-repl ()
  "Start the REPL if it is not already running; return its process."
  (unless (birnam--repl-process)
    (save-window-excursion (run-birnam)))
  (birnam--repl-process))

(defun birnam-eval-region (beg end)
  "Send the region BEG..END to the Birnam REPL.
The REPL reads Birnam source directly and accumulates multi-line
forms until the brackets balance, so the region is sent verbatim
rather than filed in as a unit."
  (interactive "r")
  (let ((proc (birnam--ensure-repl))
        (text (buffer-substring-no-properties beg end)))
    (comint-send-string proc (concat (string-trim-right text) "\n"))
    (display-buffer birnam-repl-buffer-name)))

(defun birnam-eval-buffer ()
  "Send the whole buffer to the Birnam REPL."
  (interactive)
  (birnam-eval-region (point-min) (point-max)))

(defun birnam-eval-line ()
  "Send the current line to the Birnam REPL as a single expression."
  (interactive)
  (let ((proc (birnam--ensure-repl))
        (line (string-trim
               (buffer-substring-no-properties
                (line-beginning-position) (line-end-position)))))
    (unless (string-empty-p line)
      (comint-send-string proc (concat line "\n"))
      (display-buffer birnam-repl-buffer-name))))

;;;; Keymap --------------------------------------------------------------

(defvar birnam-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-r") #'birnam-eval-region)
    (define-key map (kbd "C-c C-b") #'birnam-eval-buffer)
    (define-key map (kbd "C-c C-l") #'birnam-eval-line)
    (define-key map (kbd "C-c C-z") #'run-birnam)
    map)
  "Keymap for `birnam-mode'.")

;;;; Mode definition -----------------------------------------------------

;;;###autoload
(define-derived-mode birnam-mode prog-mode "Birnam"
  "Major mode for editing Birnam source files.

\\{birnam-mode-map}"
  :syntax-table birnam-mode-syntax-table
  (setq-local font-lock-defaults '(birnam-font-lock-keywords))
  (setq-local comment-start "// ")
  (setq-local comment-start-skip "//+[ \t]*")
  (setq-local comment-end "")
  (setq-local indent-line-function #'birnam-indent-line)
  (setq-local electric-indent-chars
              (append '(?\] ?} ?\)) electric-indent-chars)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.birnam\\'" . birnam-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("\\.bim\\'" . birnam-mode))

(provide 'birnam-mode)
;;; birnam-mode.el ends here
