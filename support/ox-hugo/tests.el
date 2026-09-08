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
                    "A [[/tags/example.html][site link]] stays a URL.\n\n"
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
              (should (string-match-p (regexp-quote "/tags/example.html") output))
              (should (string-match-p (regexp-quote "slug: \"stable-source\"") output))
              (should (string-match-p (regexp-quote "previous_slugs: [\"old-source\"]") output))
              (should-not (string-match-p (regexp-quote directory) output))
              (should-not (string-match-p "relref" output)))))
      (dolist (file (list source target))
        (when-let* ((buffer (get-file-buffer file))) (kill-buffer buffer)))
      (delete-directory directory t))))

(ert-deftest berlin-attachments-keep-bytes-and-avoid-filename-collisions ()
  (let* ((directory (make-temp-file "berlin-attachments-" t))
         (info (list :hugo-base-dir directory))
         (one (expand-file-name "one/data.csv" directory))
         (two (expand-file-name "two/data.csv" directory))
         (plain (expand-file-name "download without extension" directory)))
    (unwind-protect
        (progn
          (make-directory (file-name-directory one) t)
          (make-directory (file-name-directory two) t)
          (with-temp-file one (insert "first"))
          (with-temp-file two (insert "second"))
          (with-temp-file plain (insert "download"))
          (let ((first-url (berlin-publish-attachment one info))
                (second-url (berlin-publish-attachment two info)))
            (should-not (equal first-url second-url))
            (should (equal first-url (berlin-publish-attachment one info)))
            (dolist (pair (list (cons one first-url) (cons two second-url)
                               (cons plain (berlin-publish-attachment plain info))))
              (should (string-prefix-p "/attachments/" (cdr pair)))
              (should (equal (berlin-file-sha256 (car pair))
                             (berlin-file-sha256
                              (expand-file-name
                               (concat "static" (url-unhex-string (cdr pair))) directory))))))
          (should (equal "https://example.com/image.png"
                         (berlin-publish-attachment "https://example.com/image.png" info)))
          (should-error (berlin-publish-attachment (expand-file-name "missing.pdf" directory) info)))
      (delete-directory directory t))))

(ert-deftest berlin-preview-is-metadata-and-publishes-its-attachment ()
  (let* ((directory (make-temp-file "berlin-preview-" t))
         (source (expand-file-name "source.org" directory))
         (image (expand-file-name "preview.svg" directory)))
    (unwind-protect
        (progn
          (with-temp-file image (insert "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"))
          (with-temp-file source
            (insert ":PROPERTIES:\n:ID: preview-source\n:END:\n#+TITLE: Preview\n"
                    "#+BERLIN_PREVIEW: preview.svg\n"
                    "#+BERLIN_PREVIEW_ALT: A diagram: \"input\" & output\n"
                    "#+BERLIN_PREVIEW_SIZE: 320 224\n\nOnly body text.\n"))
          (let ((command-line-args-left (list directory "notes" source)))
            (berlin-ox-hugo-command nil))
          (let* ((relative (concat "attachments/" (berlin-file-sha256 image) "/preview.svg"))
                 (copy (expand-file-name (concat "static/" relative) directory)))
            (should (equal (berlin-file-sha256 image) (berlin-file-sha256 copy)))
            (with-temp-buffer
              (insert-file-contents (expand-file-name "content/notes/source.md" directory))
              (should (search-forward (concat "source: \"/" relative "\"") nil t))
              (goto-char (point-min))
              (should (search-forward "width: 320" nil t))
              (should (search-forward "height: 224" nil t))
              (should (search-forward "Only body text." nil t))
              (should-not (string-match-p "!\\[" (buffer-string))))))
      (when-let* ((buffer (get-file-buffer source))) (kill-buffer buffer))
      (delete-directory directory t))))

(ert-deftest berlin-preview-requires-complete-metadata ()
  (dolist (keywords '("#+BERLIN_PREVIEW: missing.svg\n"
                      "#+BERLIN_PREVIEW_ALT: Orphan description\n"
                      "#+BERLIN_PREVIEW: missing.svg\n#+BERLIN_PREVIEW_ALT: Example\n#+BERLIN_PREVIEW_SIZE: 0 224\n"))
    (with-temp-buffer
      (org-mode)
      (insert keywords)
      (should-error (berlin-ox-hugo-add-preview nil 'hugo)))))

;;; tests.el ends here
