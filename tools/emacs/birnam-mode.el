;;; birnam-mode.el --- Major mode for the Birnam language -*- lexical-binding: t; -*-

;; Author: Birnam contributors
;; Keywords: languages, birnam
;; Version: 0.3.0
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

(defun birnam--code-end-of-line ()
  "Position just past the last code character on the current line.
Trailing whitespace and any `//' line comment are excluded; a
whole-line comment reports its own beginning-of-line.  Point is left
unchanged."
  (save-excursion
    (end-of-line)
    (let ((ppss (syntax-ppss)))
      (when (nth 4 ppss)                    ; inside a line comment
        (goto-char (nth 8 ppss))))          ; back up to where it starts
    (skip-chars-backward " \t")
    (point)))

(defun birnam--prev-code-line-ends-statement-p ()
  "Non-nil if the nearest preceding code line ends a statement.
A statement is terminated by `;'.  Blank lines and whole-line comments
are skipped.  Returns non-nil when there is no preceding code line, so
the first statement in a buffer sits at column 0.  Assumes point is at
the current line's beginning."
  (save-excursion
    (catch 'done
      (while (not (bobp))
        (forward-line -1)
        (let ((code-end (birnam--code-end-of-line)))
          (when (> code-end (line-beginning-position))
            (throw 'done (eq (char-before code-end) ?\;)))))
      t)))

(defun birnam--send-first-keyword-colon (open limit)
  "Column of the colon of the FIRST keyword selector part sitting
immediately inside the bracket opened at OPEN, considering only text
before LIMIT.  Return nil when no keyword part occurs before LIMIT.
The receiver, nested brackets, binary operators, and `:=' assignment
targets are skipped, so the column returned is the Obj-C-style anchor
that later keyword parts align their own colons under."
  (save-excursion
    (goto-char (1+ open))
    (let (col)
      (while (and (not col) (< (point) limit))
        (skip-chars-forward " \t\r\n" limit)
        (forward-comment (point-max))
        (cond
         ((>= (point) limit) (setq col 'none))
         ;; `foo :=' assignment target — the colon belongs to `:=', not a selector
         ((looking-at "[A-Za-z_][A-Za-z0-9_]*[ \t]*:=")
          (goto-char (match-end 0)))
         ;; `foo:' keyword selector part (colon not itself part of `:=')
         ((looking-at "\\([A-Za-z_][A-Za-z0-9_]*\\):\\([^=]\\|$\\)")
          (goto-char (match-end 1))            ; move onto the colon itself
          (setq col (current-column)))
         ;; opening bracket — skip the whole balanced form at once
         ((looking-at "\\s(")
          (condition-case nil (forward-sexp) (error (goto-char limit))))
         ;; receiver / argument atom / operator — skip one form (or one char)
         (t
          (condition-case nil (forward-sexp) (error (forward-char 1))))))
      (and (integerp col) col))))

(defun birnam--calculate-indent ()
  "Return the target indentation column for the current line, or nil.
Nil means \"leave the line alone\" (it continues a multi-line string).

The model is relative-to-the-enclosing-bracket, hanging style: a line
is indented `birnam-indent-offset' past the line that OPENED the
bracket it sits in — not by absolute nesting depth — so a send that
wraps before opening a bracket no longer snaps back to column 0.  A
line that begins by closing a bracket lines up with the line that
opened it.  At the top level a line that continues an unterminated
statement (the previous code line did not end in `;') is indented one
step; a fresh statement sits at column 0.

Keyword-message continuation is Obj-C style: when a line begins with a
keyword selector part (`put:') and an EARLIER keyword part of the same
send already exists, the line is indented so its colon aligns under the
first keyword's colon.  The first keyword part itself (no earlier
keyword) falls back to the one-step hang, as do lines whose alignment
would run off the left margin.  Assumes point is anywhere on the line;
it is moved to BOL."
  (beginning-of-line)
  (let* ((ppss (syntax-ppss))
         (open (nth 1 ppss)))                 ; innermost containing bracket, or nil
    (cond
     ((nth 3 ppss) nil)                        ; inside a string literal
     ((null open)                              ; top level
      (if (birnam--prev-code-line-ends-statement-p) 0 birnam-indent-offset))
     (t                                        ; inside a bracket
      (let* ((starts-with-closer (looking-at "[ \t]*\\s)"))
             (kw (and (not starts-with-closer)
                      (looking-at "[ \t]*\\([A-Za-z_][A-Za-z0-9_]*\\):\\([^=]\\|$\\)")
                      (match-string 1)))
             (bol (line-beginning-position))
             (hang (save-excursion
                     (goto-char open)
                     (+ (current-indentation) birnam-indent-offset))))
        (cond
         (starts-with-closer
          (save-excursion (goto-char open) (current-indentation)))
         (kw
          (let ((anchor (birnam--send-first-keyword-colon open bol)))
            (if (and anchor (>= (- anchor (length kw)) 0))
                (- anchor (length kw))        ; align this colon under the first
              hang)))                          ; first keyword, or would underflow
         (t hang)))))))

(defun birnam-indent-line ()
  "Indent the current line for `birnam-mode'.
See `birnam--calculate-indent' for the model.  Re-indent (TAB) is
idempotent.  A wrapped keyword message aligns Obj-C style on TAB — each
continuation keyword's colon snaps under the first keyword's colon.
That alignment is anchored to the receiver line's width, so renaming
the receiver needs a re-TAB of the continuation lines to re-flow."
  (let ((indent (save-excursion (birnam--calculate-indent))))
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
  ;; Indentation aligns keyword colons by column, so it must be spaces —
  ;; a tab would only land on the anchor column when `tab-width' happened
  ;; to match, breaking the alignment everywhere else.
  (setq-local indent-tabs-mode nil)
  (setq-local indent-line-function #'birnam-indent-line)
  (setq-local electric-indent-chars
              (append '(?\] ?} ?\)) electric-indent-chars)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.birnam\\'" . birnam-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("\\.bim\\'" . birnam-mode))

(provide 'birnam-mode)
;;; birnam-mode.el ends here
