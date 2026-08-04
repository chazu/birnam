;;; birnam-mode.el --- Major mode for the Birnam language -*- lexical-binding: t; -*-

;; Author: Birnam contributors
;; Keywords: languages, birnam
;; Version: 0.4.0
;; Package-Requires: ((emacs "26.1"))

;;; Commentary:

;; A basic major mode for editing Birnam source (`.birnam') files, plus an
;; inferior REPL via comint (`run-birnam') and optional lsp-mode integration.
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
;;   assignment :=  map arrow ->  block pipe |>  result thread =>
;;   statement separator ;
;;
;; Install:
;;   (add-to-list 'load-path "/path/to/birnam/tools/emacs")
;;   (require 'birnam-mode)
;;
;; Set the binary if `birnam' is not on PATH:
;;   (setq birnam-program "/path/to/birnam/.local/bin/birnam")
;;
;; lsp-mode is started automatically when it is loaded:
;;   (setq birnam-lsp-program "/path/to/birnam/.local/bin/birnam")

;;; Code:

(require 'comint)
(require 'compile)
(require 'project)

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

(defcustom birnam-project-profile "debug"
  "Build profile used by Birnam project build, run, and test commands."
  :type '(choice (const "debug") (const "release"))
  :group 'birnam)

(defcustom birnam-lsp-program birnam-program
  "Path to the Birnam executable used by lsp-mode."
  :type 'string
  :group 'birnam)

(defcustom birnam-lsp-args '("--lsp")
  "Arguments passed to `birnam-lsp-program' for the LSP server."
  :type '(repeat string)
  :group 'birnam)

(defcustom birnam-lsp-automatic-start t
  "When non-nil, start lsp-mode when a Birnam buffer is opened."
  :type 'boolean
  :group 'birnam)

;;;; Faces / font-lock ---------------------------------------------------

(defconst birnam--keywords
  '("let" "nil" "true" "false" "self")
  "Reserved words and built-in literals.
`let' opens a binding; `nil'/`true'/`false' are literals lowered by
codegen; `self' is the bound method receiver.  There is no `super'.")

(defun birnam--match-thread-marker (limit)
  "Find the next structural `=>' marker before LIMIT.
Exact reservation matters here: do not color the data symbol `#=>' or a
longer binary selector such as `=>+'.  Markers require following spacing or
a `//' comment, matching the language grammar."
  (catch 'found
    (while (search-forward "=>" limit t)
      (let* ((start (- (point) 2))
             (finish (point))
             (before (char-before start))
             (after (char-after))
             (next (char-after (1+ (point))))
             (ppss (save-excursion (syntax-ppss start))))
        (when (and (not (nth 3 ppss))
                   (not (nth 4 ppss))
                   (not (eq before ?#))
                   (or (memq after '(?\s ?\t ?\r ?\n))
                       (and (eq after ?/) (eq next ?/))))
          (goto-char finish)
          (set-match-data (list start finish))
          (throw 'found t))))
    nil))

(defconst birnam-font-lock-keywords
  (let ((kw (regexp-opt birnam--keywords 'symbols)))
    `(;; reserved words / literals
      (,kw . font-lock-keyword-face)
      ;; structural result-thread marker, excluding #=> and longer binops
      (birnam--match-thread-marker . font-lock-builtin-face)
      ;; operators: assignment `:=', map-entry arrow `->', block pipe `|>'
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

(defun birnam--project-root (&optional directory)
  "Return the nearest directory containing `birnam.toml', or nil.
DIRECTORY defaults to the current buffer's directory and then
`default-directory'."
  (let* ((candidate (expand-file-name
                     (or directory
                         (and (buffer-file-name)
                              (expand-file-name (buffer-file-name)))
                         default-directory)))
         (dir (if (file-directory-p candidate)
                  (file-name-as-directory candidate)
                (file-name-directory candidate))))
    (when dir
      (locate-dominating-file dir "birnam.toml"))))

(defun birnam--project-try (directory)
  "Recognize a Birnam project for Emacs `project.el'."
  (let ((root (birnam--project-root directory)))
    (when root
      (cons 'birnam (file-name-as-directory (expand-file-name root))))))

(cl-defmethod project-root ((project (head birnam)))
  "Return the root stored in a Birnam `project.el' object."
  (cdr project))

(add-hook 'project-find-functions #'birnam--project-try)

(defun birnam--workspace-directory ()
  "Return the project root for the current buffer, or `default-directory'."
  (file-name-as-directory
   (expand-file-name (or (birnam--project-root) default-directory))))

(defun birnam--compile-project (subcommand &optional use-profile extra-args)
  "Run Birnam SUBCOMMAND for the current project in compilation-mode.
When USE-PROFILE is non-nil, pass `birnam-project-profile'.  EXTRA-ARGS
is a list of additional arguments appended after the profile."
  (let* ((root (birnam--workspace-directory))
         (default-directory root)
         (command (mapconcat #'shell-quote-argument
                             (append (list birnam-program subcommand root)
                                     (when use-profile
                                       (list "--profile" birnam-project-profile))
                                     extra-args)
                             " ")))
    (compile command)))

;;;###autoload
(defun birnam-check-project ()
  "Run `birnam check' for the current project."
  (interactive)
  (birnam--compile-project "check"))

;;;###autoload
(defun birnam-build-project ()
  "Run `birnam build' for the current project."
  (interactive)
  (birnam--compile-project "build" t))

;;;###autoload
(defun birnam-run-project ()
  "Run the current project with `birnam run' in compilation-mode.
`--build' is passed so an unbuilt or stale project is built first
rather than failing on a missing artifact manifest, and `--no-repl'
because a compilation buffer has no terminal to host the project's
`run.repl' session.  Use `run-birnam' for an interactive REPL."
  (interactive)
  (birnam--compile-project "run" t '("--build" "--no-repl")))

;;;###autoload
(defun birnam-test-project ()
  "Run isolated project tests with `birnam test' in compilation-mode.
`--build' is passed so the tests run against current sources."
  (interactive)
  (birnam--compile-project "test" t '("--build")))

;;;; lsp-mode ------------------------------------------------------------

;; `birnam-mode.el' is lexically scoped, while lsp-mode declares this option
;; dynamically with `defcustom'.  Declare it special before the lazy lsp-mode
;; load so the temporary root binding below is valid in either load order.
(defvar lsp-auto-guess-root nil
  "Whether lsp-mode should infer a project root when none is configured.")

;; Declared without a value on purpose: lsp-mode owns the default, and giving
;; one here would leave it nil when this file happens to load first.
(defvar lsp-language-id-configuration)

(declare-function lsp "lsp-mode" (&optional arg))
(declare-function lsp-deferred "lsp-mode" ())
(declare-function lsp-register-client "lsp-mode" (client))
(declare-function make-lsp-client "lsp-mode" (&rest args))
(declare-function lsp-stdio-connection "lsp-mode" (command &optional test-command))

(defun birnam--lsp-activate (_filename mode)
  "Activate the Birnam lsp-mode client only for `birnam-mode'."
  (eq mode 'birnam-mode))

(defvar birnam--lsp-client-registered nil)

(defun birnam--register-lsp-client ()
  "Register Birnam's stdio LSP client with lsp-mode."
  (when (and (featurep 'lsp-mode)
             (fboundp 'lsp-register-client)
             (not birnam--lsp-client-registered))
    (add-to-list 'lsp-language-id-configuration '(birnam-mode . "birnam"))
    (lsp-register-client
     (make-lsp-client
      :new-connection
      (lsp-stdio-connection
       (lambda () (append (list birnam-lsp-program) birnam-lsp-args)))
      :activation-fn #'birnam--lsp-activate
      :server-id 'birnam-lsp
      :priority -1))
    (setq birnam--lsp-client-registered t)))

(with-eval-after-load 'lsp-mode
  (birnam--register-lsp-client))

;;;###autoload
(defun birnam-lsp-start ()
  "Start lsp-mode for the current Birnam buffer.

The workspace is the nearest `birnam.toml': `birnam--project-try'
teaches `project.el' about Birnam projects, and `lsp-auto-guess-root'
is set buffer-locally so lsp-mode consults that instead of prompting,
including when Emacs was launched from another directory.

Startup goes through `lsp-deferred' when lsp-mode provides it, so a
Birnam file opened invisibly — by a grep, a project search, or a
`dired' preview — does not spawn a server until the buffer is shown."
  (interactive)
  (unless (fboundp 'lsp)
    (user-error "lsp-mode is not installed or loaded"))
  (birnam--register-lsp-client)
  (setq-local lsp-auto-guess-root t)
  (if (fboundp 'lsp-deferred)
      (lsp-deferred)
    (let ((default-directory (birnam--workspace-directory)))
      (lsp))))

(defun birnam--maybe-start-lsp ()
  "Start lsp-mode when configured and available."
  (when (and birnam-lsp-automatic-start
             (fboundp 'lsp)
             (derived-mode-p 'birnam-mode))
    (birnam-lsp-start)))

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
  (let ((default-directory (birnam--workspace-directory)))
    (unless (comint-check-proc birnam-repl-buffer-name)
      (apply #'make-comint-in-buffer "birnam" birnam-repl-buffer-name
             birnam-program nil birnam-program-args)))
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
    (define-key map (kbd "C-c C-k") #'birnam-check-project)
    (define-key map (kbd "C-c C-u") #'birnam-build-project)
    (define-key map (kbd "C-c C-p") #'birnam-run-project)
    (define-key map (kbd "C-c C-t") #'birnam-test-project)
    (define-key map (kbd "C-c C-e") #'birnam-lsp-start)
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

(add-hook 'birnam-mode-hook #'birnam--maybe-start-lsp)

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.birnam\\'" . birnam-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("\\.bim\\'" . birnam-mode))

(provide 'birnam-mode)
;;; birnam-mode.el ends here
