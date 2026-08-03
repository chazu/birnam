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

;;; birnam-mode-test.el ends here
