;;; birnam-mode-test.el --- ERT tests for birnam-mode -*- lexical-binding: t; -*-

(require 'cl-lib)
(require 'ert)

;; Keep the test self-contained: this validates the client registration and
;; mode-facing contract without requiring a particular lsp-mode installation.
(defvar lsp-language-id-configuration nil)
(defvar birnam-test-lsp-registered nil)
(defvar birnam-test-lsp-started nil)

(defun make-lsp-client (&rest properties)
  properties)

(defun lsp-stdio-connection (command-function)
  command-function)

(defun lsp-register-client (client)
  (setq birnam-test-lsp-registered client))

(defvar birnam-test-lsp-deferred nil)

(defun lsp (&rest _args)
  (setq birnam-test-lsp-started t))

(defun lsp-deferred (&rest _args)
  (setq birnam-test-lsp-deferred t))

(provide 'lsp-mode)

(load-file (expand-file-name "birnam-mode.el"
                            (file-name-directory (or load-file-name
                                                     default-directory))))

(ert-deftest birnam-mode-lsp-client-registration ()
  (should birnam--lsp-client-registered)
  (should birnam-test-lsp-registered)
  (should (eq (plist-get birnam-test-lsp-registered :server-id)
              'birnam-lsp))
  (should (equal (funcall (plist-get birnam-test-lsp-registered :new-connection))
                 (list birnam-lsp-program "--lsp")))
  (should (funcall (plist-get birnam-test-lsp-registered :activation-fn)
                   "main.birnam" 'birnam-mode))
  (should-not (funcall (plist-get birnam-test-lsp-registered :activation-fn)
                       "main.txt" 'text-mode)))

(ert-deftest birnam-mode-project-root-and-commands ()
  (let* ((root (file-name-as-directory (make-temp-file "birnam-mode-" t)))
         (nested (expand-file-name "src/nested/" root))
         (file (expand-file-name "main.birnam" nested))
         captured-command
         captured-directory)
    (unwind-protect
        (progn
          (make-directory nested t)
          (with-temp-file (expand-file-name "birnam.toml" root)
            (insert "[project]\nname = \"test\"\n"))
          (with-temp-file file
            (insert "let answer := 42\n"))
          (with-temp-buffer
            (setq default-directory nested)
            (setq buffer-file-name file)
            (let ((birnam-lsp-automatic-start nil))
              (birnam-mode)
              (should (equal (birnam--project-root) root))
              (should (equal (birnam--workspace-directory) root))
              (should (equal (project-root (birnam--project-try nested)) root))
              (should (eq (lookup-key birnam-mode-map (kbd "C-c C-k"))
                          #'birnam-check-project))
              (should (eq (lookup-key birnam-mode-map (kbd "C-c C-t"))
                          #'birnam-test-project))
              (cl-letf (((symbol-function 'compile)
                         (lambda (command)
                           (setq captured-command command)
                           (setq captured-directory default-directory)
                           command)))
                (birnam-check-project))
              (should (string-match-p (regexp-quote root) captured-command))
              (should (equal captured-directory root))
              (let ((birnam-project-profile "release"))
                (cl-letf (((symbol-function 'compile)
                           (lambda (command)
                             (setq captured-command command)
                             command)))
                  (birnam-test-project))
                (should (string-match-p "test" captured-command))
                (should (string-match-p "--profile release" captured-command))))))
      (delete-directory root t))))

(ert-deftest birnam-mode-run-and-test-build-and-avoid-tty-repl ()
  "`birnam run'/`birnam test' from a compilation buffer must build first.

An unbuilt project makes `birnam run' fail on a missing artifact
manifest, and a project whose manifest sets `run.repl = \"tty\"' would
otherwise try to open an interactive REPL in a buffer that has no
terminal, so `--no-repl' is passed too."
  (let (captured-command)
    (cl-letf (((symbol-function 'compile)
               (lambda (command) (setq captured-command command) command))
              ((symbol-function 'birnam--workspace-directory)
               (lambda () "/tmp/birnam-project/")))
      (birnam-run-project)
      (should (string-match-p " run " captured-command))
      (should (string-match-p "--build" captured-command))
      (should (string-match-p "--no-repl" captured-command))

      (setq captured-command nil)
      (birnam-test-project)
      (should (string-match-p " test " captured-command))
      (should (string-match-p "--build" captured-command))
      (should-not (string-match-p "--no-repl" captured-command))

      ;; `check' takes neither a profile nor a build flag.
      (setq captured-command nil)
      (birnam-check-project)
      (should-not (string-match-p "--build" captured-command))
      (should-not (string-match-p "--profile" captured-command)))))

(ert-deftest birnam-mode-automatic-lsp-start ()
  "Enabling the mode starts lsp-mode, deferred when lsp-mode offers it.

`lsp-deferred' is the correct entry point from a major-mode hook: a
`.birnam' file opened invisibly by a grep or project search must not
spawn a server until the buffer is actually displayed."
  (let ((birnam-test-lsp-started nil)
        (birnam-test-lsp-deferred nil)
        (birnam-lsp-automatic-start t))
    (with-temp-buffer
      (birnam-mode)
      (should birnam-test-lsp-deferred)
      (should-not birnam-test-lsp-started)
      ;; Rooting is by `birnam.toml' via `project.el', not by prompting.
      (should (local-variable-p 'lsp-auto-guess-root))
      (should lsp-auto-guess-root))))

(ert-deftest birnam-mode-lsp-prefers-the-nearest-birnam-project-root ()
  "Keep Projectile from broadening lsp-mode's root past `birnam.toml'."
  (let* ((root (file-name-as-directory (make-temp-file "birnam-mode-" t)))
         (nested (expand-file-name "src/" root))
         (file (expand-file-name "main.birnam" nested))
         (broad-root (file-name-directory (directory-file-name root)))
         (fallback (lambda (_directory) broad-root)))
    (unwind-protect
        (progn
          (make-directory nested t)
          (with-temp-file (expand-file-name "birnam.toml" root)
            (insert "[project]\nname = \"test\"\n"))
          (with-temp-buffer
            (setq default-directory nested)
            (setq buffer-file-name file)
            (let ((birnam-lsp-automatic-start nil))
              (birnam-mode)
              (setq-local projectile-project-root-functions (list fallback))
              (birnam-lsp-start)
              (should (local-variable-p
                       'projectile-project-root-functions))
              (should (eq (car projectile-project-root-functions)
                          #'birnam--projectile-root))
              (should (eq (cadr projectile-project-root-functions)
                          fallback))
              (should (equal
                       (funcall (car projectile-project-root-functions)
                                nested)
                       root)))))
      (delete-directory root t))))

(ert-deftest birnam-mode-tab-completes-after-indentation ()
  "TAB should ask completion-at-point once indentation is already correct."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (should (eq tab-always-indent 'complete))
      (should (eq (key-binding (kbd "TAB"))
                  #'indent-for-tab-command))
      (should (eq (lookup-key birnam-mode-map (kbd "<tab>"))
                  #'indent-for-tab-command)))))

(ert-deftest birnam-mode-caches-literal-receiver-completion ()
  "A prefetched receiver response should make later CAPF calls local."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (insert "[ System pri")
      (let* ((context (birnam--runtime-completion-context))
             (response (let ((result (make-hash-table :test #'equal))
                             (print (make-hash-table :test #'equal))
                             (println (make-hash-table :test #'equal)))
                         (puthash "label" "print:" print)
                         (puthash "label" "println:" println)
                         (puthash "items" (vector print println) result)
                         result)))
        (should (equal (plist-get context :receiver) "System"))
        (should (equal (plist-get context :prefix) "pri"))
        (birnam--record-completions "System" "" response)
        (pcase-let ((`(,beg ,end ,labels . ,_)
                     (birnam-cached-completion-at-point)))
          (should (= beg 10))
          (should (= end (point)))
          (should (equal labels '("print:" "println:"))))))))

(ert-deftest birnam-mode-caches-global-receiver-completion ()
  "A prefetched global snapshot should complete `[ Syst' locally."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (insert "[ Syst")
      (let* ((context (birnam--global-completion-context))
             (response (let ((result (make-hash-table :test #'equal))
                             (system (make-hash-table :test #'equal))
                             (symbol (make-hash-table :test #'equal)))
                         (puthash "label" "System" system)
                         (puthash "label" "Symbol" symbol)
                         (puthash "items" (vector system symbol) result)
                         result)))
        (should (equal (plist-get context :cache-key) :globals))
        (should (equal (plist-get context :prefix) "Syst"))
        (birnam--record-completions :globals "" response)
        (pcase-let ((`(,beg ,end ,labels . ,_)
                     (birnam-cached-completion-at-point)))
          (should (= beg 3))
          (should (= end (point)))
          (should (equal labels '("System"))))))))

(ert-deftest birnam-mode-cached-runtime-completion-stays-fast ()
  "Method and global cache lookups should remain safely below interactive latency."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (let ((response (let ((result (make-hash-table :test #'equal))
                            (system (make-hash-table :test #'equal))
                            (println (make-hash-table :test #'equal)))
                        (puthash "label" "System" system)
                        (puthash "label" "println:" println)
                        (puthash "items" (vector system println) result)
                        result)))
        (birnam--record-completions :globals "" response)
        (birnam--record-completions "System" "" response)
        (let ((started (float-time)))
          (dotimes (_ 1000)
            (erase-buffer)
            (insert "[ Syst")
            (should (equal (nth 2 (birnam-cached-completion-at-point))
                           '("System")))
            (erase-buffer)
            (insert "[ System pri")
            (should (equal (nth 2 (birnam-cached-completion-at-point))
                           '("println:"))))
          ;; Two thousand full context parses and cache lookups should average
          ;; under 0.5 ms even with a generous cross-machine budget.
          (should (< (- (float-time) started) 1.0)))))))

(ert-deftest birnam-mode-installs-cache-ahead-of-existing-lsp-capfs ()
  "Receiver cache must survive and precede general LSP CAPF setup."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (setq-local completion-at-point-functions '(birnam-test-generic-capf))
      (birnam--lsp-completion-cache-setup)
      (should (equal completion-at-point-functions
                     '(birnam-cached-completion-at-point
                       birnam-test-generic-capf))))))

(ert-deftest birnam-mode-prefetch-waits-for-lsp-initialization ()
  "A startup timer must not request completion before capabilities arrive."
  (let ((birnam-lsp-automatic-start nil)
        (request-called nil)
        (rescheduled nil))
    (with-temp-buffer
      (birnam-mode)
      (insert "[ Syst")
      (setq-local lsp-mode t)
      (cl-letf (((symbol-function 'lsp-workspaces)
                 (lambda () '(birnam-test-workspace)))
                ((symbol-function 'lsp--workspace-status)
                 (lambda (_workspace) 'starting))
                ((symbol-function 'lsp--text-document-position-params)
                 (lambda () 'birnam-test-params))
                ((symbol-function 'lsp-request-async)
                 (lambda (&rest _args) (setq request-called t)))
                ((symbol-function 'birnam--schedule-completion-prefetch)
                 (lambda () (setq rescheduled t))))
        (birnam--start-completion-prefetch (current-buffer)))
      (should-not request-called)
      (should rescheduled)
      (should-not (gethash :globals birnam--completion-prefetches)))))

(ert-deftest birnam-mode-prefetch-clears-a-synchronous-lsp-error ()
  "A preflight LSP error must not escape its timer or poison the cache key."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (insert "[ Syst")
      (setq-local lsp-mode t)
      (cl-letf (((symbol-function 'lsp-workspaces)
                 (lambda () '(birnam-test-workspace)))
                ((symbol-function 'lsp--workspace-status)
                 (lambda (_workspace) 'initialized))
                ((symbol-function 'lsp--capability)
                 (lambda (_capability) t))
                ((symbol-function 'lsp--text-document-position-params)
                 (lambda () 'birnam-test-params))
                ((symbol-function 'lsp-request-async)
                 (lambda (&rest _args)
                   (error "Birnam test startup race"))))
        (should
         (eq (condition-case nil
                 (progn
                   (birnam--start-completion-prefetch (current-buffer))
                   'returned)
               (error 'escaped))
             'returned)))
      (should-not (gethash :globals birnam--completion-prefetches)))))

(ert-deftest birnam-mode-empty-cached-prefix-does-not-fall-through ()
  "A known receiver with no matching selector must not call a slow CAPF."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (insert "[ System definitelyNoSuchSelector")
      (let ((response (let ((result (make-hash-table :test #'equal)))
                        (puthash "items" [] result)
                        result))
            (fallback-called nil))
        (birnam--record-completions "System" "" response)
        (setq-local
         completion-at-point-functions
         (list #'birnam-cached-completion-at-point
               (lambda ()
                 (setq fallback-called t)
                 nil)))
        (completion-at-point)
        (should-not fallback-called)))))

(ert-deftest birnam-mode-empty-global-prefix-does-not-fall-through ()
  "A known global snapshot with no matching name must not call a slow CAPF."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (insert "[ DefinitelyNoSuchGlobal")
      (let ((response (let ((result (make-hash-table :test #'equal)))
                        (puthash "items" [] result)
                        result))
            (fallback-called nil))
        (birnam--record-completions :globals "" response)
        (setq-local
         completion-at-point-functions
         (list #'birnam-cached-completion-at-point
               (lambda ()
                 (setq fallback-called t)
                 nil)))
        (completion-at-point)
        (should-not fallback-called)))))

(ert-deftest birnam-mode-bounds-runtime-completion-prefixes ()
  "Edited prefixes must not grow a receiver cache without bound."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (dotimes (i 12)
        (let ((response (let ((result (make-hash-table :test #'equal))
                              (item (make-hash-table :test #'equal)))
                          (puthash "label" (format "p%d" i) item)
                          (puthash "items" (vector item) result)
                          result)))
          (birnam--record-completions "System" (format "p%d" i) response)))
      (should (= (length (gethash "System" birnam--completion-cache)) 8)))))

(ert-deftest birnam-mode-lsp-start-falls-back-without-deferred ()
  "Older lsp-mode builds without `lsp-deferred' still start directly."
  (let ((birnam-test-lsp-started nil)
        (birnam-test-lsp-deferred nil))
    (let ((saved (symbol-function 'lsp-deferred)))
      (unwind-protect
          (progn
            (fmakunbound 'lsp-deferred)
            (with-temp-buffer
              (birnam-mode)
              (should birnam-test-lsp-started)
              (should-not birnam-test-lsp-deferred)))
        (fset 'lsp-deferred saved)))
    (should (fboundp 'lsp-deferred))))

(ert-deftest birnam-mode-automatic-lsp-start-can-be-disabled ()
  (let ((birnam-test-lsp-started nil)
        (birnam-test-lsp-deferred nil)
        (birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (birnam-mode)
      (should-not birnam-test-lsp-started)
      (should-not birnam-test-lsp-deferred))))

(ert-deftest birnam-mode-lsp-root-binding-is-special ()
  "Keep the lazy lsp-mode binding dynamically scoped.

The mode file uses lexical binding, but lsp-mode declares this option with
`defcustom' when it is loaded.  It must be declared special here too so the
two files can be loaded in either order without a lexical/dynamic conflict."
  (should (special-variable-p 'lsp-auto-guess-root)))

(ert-deftest birnam-mode-highlights-only-the-exact-thread-marker ()
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (insert "[x => foo]\n#=>\n[x =>+ 1]\n")
      (birnam-mode)
      (font-lock-ensure)
      (goto-char (point-min))
      (search-forward "=>")
      (should (eq (get-text-property (- (point) 2) 'face)
                  'font-lock-builtin-face))
      (search-forward "#=>")
      (should (eq (get-text-property (- (point) 2) 'face)
                  'font-lock-constant-face))
      (search-forward "=>+")
      (should-not (eq (get-text-property (- (point) 3) 'face)
                      'font-lock-builtin-face)))))

(ert-deftest birnam-mode-indents-marker-leading-thread-legs ()
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (insert "[Map\n=> new\n=> at: #a put: 1]\n")
      (birnam-mode)
      (indent-region (point-min) (point-max))
      (should (equal (buffer-string)
                     "[Map\n  => new\n  => at: #a put: 1]\n")))))

(ert-deftest birnam-mode-keeps-semicolon-free-top-level-forms-unindented ()
  "Whitespace-separated top-level forms are statements, not continuations."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (insert "// No semicolons are required.\n\n"
              "[System println: \"first\"]\n\n"
              "let main := [| [System println: \"second\"] ]\n\n"
              "[main value]\n")
      (birnam-mode)
      (indent-region (point-min) (point-max))
      (should (equal (buffer-string)
                     (concat "// No semicolons are required.\n\n"
                             "[System println: \"first\"]\n\n"
                             "let main := [| [System println: \"second\"] ]\n\n"
                             "[main value]\n"))))))

(ert-deftest birnam-mode-indents-a-let-value-after-assignment ()
  "A value moved below its `:=' remains a top-level continuation."
  (let ((birnam-lsp-automatic-start nil))
    (with-temp-buffer
      (insert "let main :=\n[|\n[System println: \"hello\"]\n]\n"
              "[main value]\n")
      (birnam-mode)
      (indent-region (point-min) (point-max))
      (should (equal (buffer-string)
                     (concat "let main :=\n"
                             "  [|\n"
                             "    [System println: \"hello\"]\n"
                             "  ]\n"
                             "[main value]\n"))))))

;;; birnam-mode-test.el ends here
