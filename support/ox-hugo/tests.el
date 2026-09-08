;;; tests.el --- Isolated export contract tests -*- lexical-binding: t; -*-

(require 'ert)
(load (expand-file-name "export.el" (file-name-directory load-file-name)))

(ert-deftest berlin-export-preserves-document-and-heading-identity ()
  (let* ((directory (make-temp-file "berlin-reference-export-" t))
         (source (expand-file-name "source.org" directory))
         (target (expand-file-name "target.org" directory))
         (origins (expand-file-name "origins.json" directory))
         (process-environment (copy-sequence process-environment))
         (translator (symbol-function 'org-hugo-heading))
         (org-id-locations nil))
    (unwind-protect
        (progn
          (setenv "BERLIN_ORG_SOURCE_MAP" origins)
          (with-temp-file source
            (insert ":PROPERTIES:\n:ID: source-id\n:END:\n#+TITLE: Source\n#+HUGO_SLUG: stable-source\n#+HUGO_CUSTOM_FRONT_MATTER: :comments true :previous_slugs '(\"old-source\")\n\n"
                    "Builds on [[id:target-id][Target]] and [[id:heading-id][Details]].\n\n"
                    "An unresolved [[id:unpublished-id][reference]] stays inspectable.\n\n"
                    "#+begin_src emacs-lisp :exports results\n(error \"Babel must not execute during export\")\n#+end_src\n"))
          (with-temp-file target
            (insert ":PROPERTIES:\n:ID: target-id\n:END:\n#+TITLE: Target\n\n* Details\n:PROPERTIES:\n:ID: heading-id\n:CUSTOM_ID: details\n:END:\nBody.\n"))
          (let ((command-line-args-left (list directory "notes" source target)))
            (berlin-ox-hugo-command nil))
          (should (eq translator (symbol-function 'org-hugo-heading)))
          (let* ((data (with-temp-buffer
                         (insert-file-contents origins)
                         (json-parse-buffer :object-type 'alist :array-type 'list :null-object nil)))
                 (export (cadr (alist-get 'documents data)))
                 (heading (car (alist-get 'headings export))))
            (should (= 1 (alist-get 'schema_version data)))
            (should (equal "target-id" (alist-get 'document export)))
            (should (file-equal-p target (alist-get 'source export)))
            (should (equal (berlin-file-sha256 target) (alist-get 'source_hash export)))
            (should (equal (berlin-file-sha256 (alist-get 'markdown export))
                           (alist-get 'markdown_hash export)))
            (should (equal "details" (alist-get 'fragment heading)))
            (should (equal "heading-id" (alist-get 'id (alist-get 'heading heading))))
            (should (equal '("Details") (alist-get 'outline (alist-get 'heading heading)))))
          (with-temp-buffer
            (insert-file-contents (expand-file-name "content/notes/source.md" directory))
            (let ((output (buffer-string)))
              (should (string-match-p (regexp-quote "[Target](id:target-id)") output))
              (should (string-match-p (regexp-quote "[Details](id:target-id#details)") output))
              (should (string-match-p (regexp-quote "[reference](id:unpublished-id)") output))
              (should (string-match-p "comments: true" output))
              (should (string-match-p (regexp-quote "slug: \"stable-source\"") output))
              (should (string-match-p (regexp-quote "previous_slugs: [\"old-source\"]") output))
              (should-not (string-match-p (regexp-quote directory) output))
              (should-not (string-match-p "relref" output)))))
      (dolist (file (list source target))
        (when-let* ((buffer (get-file-buffer file))) (kill-buffer buffer)))
      (delete-directory directory t))))

;;; tests.el ends here
