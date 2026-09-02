;;; birnam-mode.el --- Major mode for the Birnam language -*- lexical-binding: t; -*-

;; Author: Birnam contributors
;; Keywords: languages, birnam
;; Version: 0.4.4
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
(require 'seq)
(require 'subr-x)

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

(defcustom birnam-lsp-completion-prefetch-delay 0.15
  "Idle seconds before prefetching literal receiver methods.
Set this to nil to disable background prefetch; successful explicit requests
are still retained in the process-stable local cache."
  :type '(choice (const :tag "Disabled" nil) number)
  :group 'birnam)

;;;; LSP runtime completion cache ---------------------------------------

(defvar-local birnam--completion-cache nil)
(defvar-local birnam--completion-prefetches nil)
(defvar-local birnam--completion-prefetch-timer nil)

(defun birnam--runtime-completion-context (&optional position)
  "Return literal receiver completion context at POSITION, or nil."
  (save-excursion
    (when position (goto-char position))
    (let* ((line-start (line-beginning-position))
           (text (buffer-substring-no-properties line-start (point)))
           (pattern
            ".*\\[\\s-*\\([[:alpha:]_][[:alnum:]_]*\\)\\s-+\\([[:alnum:]_]*\\)\\'"))
      (when (string-match pattern text)
        (list :receiver (match-string 1 text)
              :prefix (match-string 2 text)
              :start (+ line-start (match-beginning 2)))))))

(defun birnam--completion-items (response)
  "Return completion items carried by LSP RESPONSE."
  (cond
   ((hash-table-p response) (gethash "items" response))
   ((vectorp response) (append response nil))
   ((listp response) response)))

(defun birnam--record-completions (receiver prefix response)
  "Cache labels from RESPONSE for RECEIVER beginning with PREFIX."
  (let ((labels
         (delete-dups
          (delq nil
                (mapcar
                 (lambda (item)
                   (let ((label
                          (cond
                           ((hash-table-p item) (gethash "label" item))
                           ((listp item) (or (plist-get item :label)
                                             (alist-get 'label item))))))
                     (and (stringp label) (substring-no-properties label))))
                 (birnam--completion-items response))))))
    (when labels
      (unless (hash-table-p birnam--completion-cache)
        (setq birnam--completion-cache (make-hash-table :test #'equal)))
      (let* ((buckets (gethash receiver birnam--completion-cache))
             (buckets (cons (cons prefix labels)
                            (assoc-delete-all prefix buckets))))
        ;; Different disjoint prefixes are useful, but never let edited text
        ;; grow this process-stable cache without bound.
        (when (> (length buckets) 8)
          (setcdr (nthcdr 7 buckets) nil))
        (puthash receiver buckets birnam--completion-cache)))))

(defun birnam--cached-labels (receiver prefix)
  "Return cached RECEIVER labels applicable to PREFIX."
  (when (hash-table-p birnam--completion-cache)
    (let (best-prefix best-labels)
      (dolist (bucket (gethash receiver birnam--completion-cache))
        (when (and (string-prefix-p (car bucket) prefix)
                   (or (null best-prefix)
                       (> (length (car bucket)) (length best-prefix))))
          (setq best-prefix (car bucket)
                best-labels (cdr bucket))))
      (seq-filter (lambda (label) (string-prefix-p prefix label)) best-labels))))

(defun birnam-cached-completion-at-point ()
  "Complete a literal Birnam receiver from the process-stable local cache."
  (when-let* ((context (birnam--runtime-completion-context))
              (receiver (plist-get context :receiver))
              (prefix (plist-get context :prefix))
              (labels (birnam--cached-labels receiver prefix)))
    (list (plist-get context :start) (point) labels :exclusive 'no)))

(defun birnam--prefetch-finished (buffer receiver response)
  "Record RECEIVER RESPONSE for BUFFER and clear its in-flight marker."
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when (hash-table-p birnam--completion-prefetches)
        (remhash receiver birnam--completion-prefetches))
      ;; The request starts at the selector, so the response contains the
      ;; receiver's complete method set and applies to every later prefix.
      (birnam--record-completions receiver "" response))))

(defun birnam--prefetch-failed (buffer receiver _error)
  "Clear BUFFER's in-flight RECEIVER marker after a failed request."
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (when (hash-table-p birnam--completion-prefetches)
        (remhash receiver birnam--completion-prefetches)))))

(defun birnam--start-completion-prefetch (buffer)
  "Start a non-blocking receiver completion request for BUFFER."
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (setq birnam--completion-prefetch-timer nil)
      (when (and (bound-and-true-p lsp-mode) (lsp-workspaces))
        (when-let* ((context (birnam--runtime-completion-context))
                    (receiver (plist-get context :receiver)))
          (unless (or (birnam--cached-labels receiver "")
                      (and (hash-table-p birnam--completion-prefetches)
                           (gethash receiver birnam--completion-prefetches)))
            (unless (hash-table-p birnam--completion-prefetches)
              (setq birnam--completion-prefetches
                    (make-hash-table :test #'equal)))
            (puthash receiver t birnam--completion-prefetches)
            (let ((params
                   (save-excursion
                     (goto-char (plist-get context :start))
                     (lsp--text-document-position-params))))
              (lsp-request-async
               "textDocument/completion" params
               (lambda (response)
                 (birnam--prefetch-finished buffer receiver response))
               :error-handler
               (lambda (error)
                 (birnam--prefetch-failed buffer receiver error))
               :mode 'detached))))))))

(defun birnam--schedule-completion-prefetch ()
  "Schedule non-blocking completion prefetch for the receiver at point."
  (when (timerp birnam--completion-prefetch-timer)
    (cancel-timer birnam--completion-prefetch-timer))
  (setq birnam--completion-prefetch-timer nil)
  (when (and birnam-lsp-completion-prefetch-delay
             (birnam--runtime-completion-context))
    (setq birnam--completion-prefetch-timer
          (run-with-idle-timer birnam-lsp-completion-prefetch-delay nil
                              #'birnam--start-completion-prefetch
                              (current-buffer)))))

(defun birnam--record-sync-completion (original method params)
  "Call ORIGINAL with METHOD PARAMS and retain Birnam completion labels."
  (let ((context (and (derived-mode-p 'birnam-mode)
                      (equal method "textDocument/completion")
                      (birnam--runtime-completion-context))))
    (let ((response (funcall original method params)))
      (when context
        (birnam--record-completions
         (plist-get context :receiver)
         (plist-get context :prefix)
         response))
      response)))

(defun birnam--lsp-completion-cache-setup ()
  "Put the receiver cache before lsp-mode's synchronous completion CAPF."
  (when (derived-mode-p 'birnam-mode)
    ;; A restarted workspace may expose a different runtime method set.
    (setq birnam--completion-cache (make-hash-table :test #'equal)
          birnam--completion-prefetches (make-hash-table :test #'equal))
    (add-to-list 'completion-at-point-functions
                 #'birnam-cached-completion-at-point)
    (birnam--schedule-completion-prefetch)))

(defun birnam--completion-cache-setup ()
  "Initialize bounded runtime completion state in the current buffer."
  (setq birnam--completion-cache (make-hash-table :test #'equal)
        birnam--completion-prefetches (make-hash-table :test #'equal))
  (add-hook 'post-command-hook #'birnam--schedule-completion-prefetch nil t))

(with-eval-after-load 'lsp-completion
  (unless (advice-member-p #'birnam--record-sync-completion
                           #'lsp-request-while-no-input)
    (advice-add 'lsp-request-while-no-input :around
                #'birnam--record-sync-completion))
  (add-hook 'lsp-completion-mode-hook #'birnam--lsp-completion-cache-setup))

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

(defun birnam--prev-code-line-continues-statement-p ()
  "Non-nil if the nearest preceding code line requires a continuation.
Blank lines and whole-line comments are skipped.  Birnam expressions are
self-delimiting and `;' is optional, so an ordinary completed expression
does not indent the next top-level line.  A trailing `:=' is the one
top-level prefix whose value may begin on a following line.  Assumes point
is at the current line's beginning."
  (save-excursion
    (catch 'done
      (while (not (bobp))
        (forward-line -1)
        (let ((code-end (birnam--code-end-of-line)))
          (when (> code-end (line-beginning-position))
            (goto-char code-end)
            (throw 'done
                   (and (>= (- code-end (line-beginning-position)) 2)
                        (equal (buffer-substring-no-properties
                                (- code-end 2) code-end)
                               ":="))))))
      nil)))

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
opened it.  Top-level forms sit at column 0 whether or not the previous
form used the optional `;' separator.  A value beginning on the line
after a trailing `:=' is indented one step.

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
      (if (birnam--prev-code-line-continues-statement-p)
          birnam-indent-offset
        0))
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

;; Projectile is one of lsp-mode's preferred project-root providers.  Declare
;; its policy variable special without loading Projectile eagerly so a Birnam
;; buffer can put its precise manifest lookup first when Projectile is present.
(defvar projectile-project-root-functions)

(declare-function lsp "lsp-mode" (&optional arg))
(declare-function lsp-deferred "lsp-mode" ())
(declare-function lsp-register-client "lsp-mode" (client))
(declare-function make-lsp-client "lsp-mode" (&rest args))
(declare-function lsp-stdio-connection "lsp-mode" (command &optional test-command))

(defun birnam--lsp-activate (_filename mode)
  "Activate the Birnam lsp-mode client only for `birnam-mode'."
  (eq mode 'birnam-mode))

(defun birnam--projectile-root (directory)
  "Return the nearest Birnam project root above DIRECTORY.
This adapter lets Projectile and lsp-mode use the same `birnam.toml'
boundary as `project.el'."
  (birnam--project-root directory))

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
teaches `project.el' about Birnam projects, a buffer-local Projectile
root function prevents lsp-mode from choosing a broader cached project,
and `lsp-auto-guess-root' avoids prompting when Emacs was launched from
another directory.

Startup goes through `lsp-deferred' when lsp-mode provides it, so a
Birnam file opened invisibly — by a grep, a project search, or a
`dired' preview — does not spawn a server until the buffer is shown."
  (interactive)
  (unless (fboundp 'lsp)
    (user-error "lsp-mode is not installed or loaded"))
  (birnam--register-lsp-client)
  (setq-local lsp-auto-guess-root t)
  (setq-local projectile-project-root-functions
              (cons #'birnam--projectile-root
                    (delq #'birnam--projectile-root
                          (and (boundp 'projectile-project-root-functions)
                               (copy-sequence
                                projectile-project-root-functions)))))
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
    (define-key map (kbd "<tab>") #'indent-for-tab-command)
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
  ;; Preserve TAB's first duty as the indentation command, but once the line
  ;; is correctly indented let it call `completion-at-point' (and therefore
  ;; lsp-mode's completion provider).
  (setq-local tab-always-indent 'complete)
  (setq-local electric-indent-chars
              (append '(?\] ?} ?\)) electric-indent-chars))
  (birnam--completion-cache-setup))

(add-hook 'birnam-mode-hook #'birnam--maybe-start-lsp)

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.birnam\\'" . birnam-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("\\.bim\\'" . birnam-mode))

(provide 'birnam-mode)
;;; birnam-mode.el ends here
