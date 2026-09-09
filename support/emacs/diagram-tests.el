;;; diagram-tests.el --- Explicit authoring contracts -*- lexical-binding: t; -*-

(require 'ert)
(require 'cl-lib)
(load (expand-file-name "berlin-diagrams.el" (file-name-directory load-file-name)))

(defmacro berlin-test-diagram-buffer (headers &rest body)
  (declare (indent 1))
  `(with-temp-buffer
     (org-mode)
     (setq buffer-file-name (expand-file-name "diagram.org" temporary-file-directory))
     (setq default-directory temporary-file-directory)
     (insert "#+begin_src latex " ,headers "\nLaTeX body\n#+end_src\n")
     (goto-char (point-min))
     ,@body))

(ert-deftest berlin-diagram-cancellation-does-not-evaluate-headers ()
  (berlin-test-diagram-buffer ":file (error \"header executed\")"
    (cl-letf (((symbol-function 'executable-find) (lambda (_) "/test/program"))
              ((symbol-function 'yes-or-no-p) (lambda (_) nil))
              ((symbol-function 'org-babel-get-src-block-info)
               (lambda (&rest _) (ert-fail "Read Babel headers before consent"))))
      (should-error (berlin-render-diagram) :type 'user-error))))

(ert-deftest berlin-diagram-requires-latex-and-the-optional-toolchain ()
  (berlin-test-diagram-buffer ":file diagram.svg"
    (cl-letf (((symbol-function 'executable-find) (lambda (_) nil)))
      (should-error (berlin-render-diagram) :type 'user-error))
    (goto-char (point-min))
    (delete-region (point-min) (point-max))
    (insert "#+begin_src emacs-lisp\n(error \"wrong block\")\n#+end_src")
    (goto-char (point-min))
    (should-error (berlin-render-diagram) :type 'user-error)))

(ert-deftest berlin-diagram-requires-local-svg-without-directory-redirection ()
  (dolist (headers '("" ":file diagram.pdf" ":file /ssh:host:/diagram.svg"
                     ":file diagram.svg :dir /tmp" ":file nonexistent-directory/diagram.svg"
                     ":file diagram.svg :eval never" ":file diagram.svg :eval no"))
    (berlin-test-diagram-buffer headers
      (cl-letf (((symbol-function 'executable-find) (lambda (_) "/test/program"))
                ((symbol-function 'yes-or-no-p) (lambda (_) t))
                ((symbol-function 'org-babel-execute-src-block)
                 (lambda (&rest _) (ert-fail "Executed an invalid destination"))))
        (should-error (berlin-render-diagram) :type 'user-error)))))

(ert-deftest berlin-diagram-scopes-toolchain-settings-and-never-saves ()
  (berlin-test-diagram-buffer ":file diagram.svg :results file graphics"
    (let ((original-compiler org-latex-pdf-process)
          (original-converter org-babel-latex-pdf-svg-process)
          (original-confirm org-confirm-babel-evaluate)
          (original-backend (symbol-function 'org-babel-execute:latex))
          called)
      (cl-letf (((symbol-function 'executable-find) (lambda (_) "/test/program"))
                ((symbol-function 'yes-or-no-p) (lambda (_) t))
                ((symbol-function 'save-buffer) (lambda (&rest _) (ert-fail "Saved Org")))
                ((symbol-function 'org-babel-execute-src-block)
                 (lambda (_ info)
                   (setq called t)
                   (should (equal "latex" (car info)))
                   (should-not org-confirm-babel-evaluate)
                   (should (eq (symbol-function 'org-babel-execute:latex) #'berlin-execute-latex-graphic))
                   (error "Compilation failed"))))
        (should-error (berlin-render-diagram)))
      (should called)
      (should (eq original-backend (symbol-function 'org-babel-execute:latex)))
      (should (equal original-compiler org-latex-pdf-process))
      (should (equal original-converter org-babel-latex-pdf-svg-process))
      (should (equal original-confirm org-confirm-babel-evaluate)))))

;;; diagram-tests.el ends here
