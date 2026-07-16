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

(defun lsp (&rest _args)
  (setq birnam-test-lsp-started t))

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
              (cl-letf (((symbol-function 'compile)
                         (lambda (command)
                           (setq captured-command command)
                           (setq captured-directory default-directory)
                           command)))
                (birnam-check-project))
              (should (string-match-p (regexp-quote root) captured-command))
              (should (equal captured-directory root)))))
      (delete-directory root t))))

(ert-deftest birnam-mode-automatic-lsp-start ()
  (let ((birnam-test-lsp-started nil)
        (birnam-lsp-automatic-start t))
    (with-temp-buffer
      (birnam-mode)
      (should birnam-test-lsp-started))))

(ert-deftest birnam-mode-lsp-root-binding-is-special ()
  "Keep the lazy lsp-mode binding dynamically scoped.

The mode file uses lexical binding, but lsp-mode declares this option with
`defcustom' when it is loaded.  It must be declared special here too so the
two files can be loaded in either order without a lexical/dynamic conflict."
  (should (special-variable-p 'lsp-auto-guess-root)))

;;; birnam-mode-test.el ends here
