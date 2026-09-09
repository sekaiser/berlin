;;; code-results-tests.el --- Non-evaluating publication contracts -*- lexical-binding: t; -*-
(require 'ert)
(require 'cl-lib)

(defun berlin-test-export-body (body)
  (let* ((directory (make-temp-file "berlin-code-results-" t))
         (source (expand-file-name "source.org" directory)))
    (unwind-protect
        (progn
          (with-temp-file source (insert ":PROPERTIES:\n:ID: results\n:END:\n#+TITLE: Results\n\n" body))
          (let ((command-line-args-left (list directory "notes" source)))
            (cl-letf (((symbol-function 'org-babel-execute-src-block) (lambda (&rest _) (ert-fail "Babel executed")))
                      ((symbol-function 'org-babel-ref-resolve) (lambda (&rest _) (ert-fail "Variable resolved"))))
              (berlin-ox-hugo-command nil)))
          (with-temp-buffer
            (insert-file-contents (expand-file-name "content/notes/source.md" directory))
            (buffer-string)))
      (when-let* ((buffer (get-file-buffer source))) (kill-buffer buffer))
      (delete-directory directory t))))

(ert-deftest berlin-export-stored-results-obeys-all-four-modes ()
  (dolist (mode '("code" "results" "both" "none"))
    (let ((output (berlin-test-export-body
                   (format "#+name: computation\n#+begin_src python :exports %s :var x=(error \"must not run\") :session active :post other()\nSOURCE_SENTINEL\n#+end_src\n\n#+RESULTS: computation\n: RESULT_SENTINEL\n\nFollowing prose.\n" mode))))
      (should (eq (not (null (string-match-p "SOURCE_SENTINEL" output)))
                  (not (null (member mode '("code" "both"))))))
      (should (eq (not (null (string-match-p "RESULT_SENTINEL" output)))
                  (not (null (member mode '("results" "both"))))))
      (should (string-match-p "Following prose" output)))))

(ert-deftest berlin-export-inherits-literal-headers-and-keeps-result-targets ()
  (let ((output (berlin-test-export-body
                 "#+PROPERTY: header-args :exports none\n\n* Visible\n:PROPERTIES:\n:header-args:python: :exports results\n:END:\nSee [[computation]].\n\n#+name: computation\n#+begin_src python\nHIDDEN_SOURCE\n#+end_src\n\n#+RESULTS: computation\n| result | 42 |\n\n#+begin_src python :exports both\nVISIBLE_SOURCE\n#+end_src\n\n#+RESULTS:\n: SAVED_RESULT\n")))
    (should-not (string-match-p "HIDDEN_SOURCE" output))
    (should (string-match-p "VISIBLE_SOURCE" output))
    (should (string-match-p "SAVED_RESULT" output))
    (should (string-match-p "42" output))))

(ert-deftest berlin-export-rejects-executable-export-selection ()
  (should-error (berlin-test-export-body
                 "#+begin_src python :exports (progn (error \"evaluated\") \"both\")\nSOURCE\n#+end_src\n")))

(ert-deftest berlin-result-only-images-retain-captions-and-alternative-text ()
  (let ((output (berlin-test-export-body
                 "#+CAPTION: A meaningful caption.\n#+ATTR_HTML: :alt A meaningful alternative.\n#+name: image\n#+begin_src latex :exports results\nHIDDEN_DRAWING\n#+end_src\n\n#+RESULTS: image\n[[https://notebook.example.org/figure.svg]]\n")))
    (should-not (string-match-p "HIDDEN_DRAWING" output))
    (should (string-match-p "A meaningful caption" output))
    (should (string-match-p "A meaningful alternative" output))))

(ert-deftest berlin-excluded-results-do-not-leak-through-blank-lines-or-raw-text ()
  (dolist (result '(": PRIVATE_RESULT\n\n\n\n" "PRIVATE_RESULT\n\n"))
    (let ((output (berlin-test-export-body
                   (concat "#+name: hidden\n#+begin_src python :exports none\nPRIVATE_CODE\n#+end_src\n\n#+RESULTS: hidden\n"
                           result "Following prose.\n"))))
      (should-not (string-match-p "PRIVATE_" output))
      (should (string-match-p "Following prose" output)))))
