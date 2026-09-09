;;; knowledge-tests.el --- Org-roam-compatible identity and attachment contracts -*- lexical-binding: t; -*-
(require 'ert)
(require 'org-attach)

(ert-deftest berlin-selected-id-index-never-scans-the-global-graph ()
  (let ((output
         (cl-letf (((symbol-function 'org-id-update-id-locations)
                    (lambda (&rest _) (ert-fail "Scanned global ID locations"))))
           (berlin-test-export-body "A reference to [[id:unpublished][a private note]] stays unresolved.\n"))))
    (should (string-match-p (regexp-quote "id:unpublished") output))))

(ert-deftest berlin-org-roam-identities-renames-and-private-attachments ()
  (let* ((directory (make-temp-file "berlin-knowledge-" t))
         (source (expand-file-name "source.org" directory))
         (target (expand-file-name "target.org" directory))
         (renamed (expand-file-name "renamed.org" directory))
         (assets (expand-file-name "files" directory))
         (org-id-locations nil)
         (org-attach-use-inheritance t))
    (unwind-protect
        (progn
          (make-directory assets)
          (with-temp-file (expand-file-name "result.svg" assets)
            (insert "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><circle cx=\"5\" cy=\"5\" r=\"4\"/></svg>"))
          (with-temp-file (expand-file-name "unreferenced.txt" assets) (insert "PRIVATE_UNUSED"))
          (with-temp-file source
            (insert ":PROPERTIES:\n:ID: source-id\n:ROAM_ALIASES: \"Source alias\"\n:END:\n#+TITLE: Source\n\n"
                    "Links to [[id:target-id][Target]] and [[id:heading-id][Detail]].\n\n"
                    "* Attachments\n:PROPERTIES:\n:DIR: files\n:END:\n** Inherited\n"
                    "#+CAPTION: A saved result.\n#+ATTR_HTML: :alt A circle.\n[[attachment:result.svg]]\n\n"
                    "* Private :noexport:\n:PROPERTIES:\n:ID: private-heading\n:DIR: missing-private-directory\n:END:\n"
                    "PRIVATE_TEXT [[attachment:secret.svg]]\n"))
          (with-temp-file target
            (insert ":PROPERTIES:\n:ID: target-id\n:ROAM_REFS: https://notebook.example.org/source\n:END:\n#+TITLE: Target\n\n"
                    "* Detail\n:PROPERTIES:\n:ID: heading-id\n:CUSTOM_ID: stable-detail\n:END:\nPublic detail.\n"))
          (dolist (destination (list target renamed))
            (when (equal destination renamed) (rename-file target renamed))
            (let ((command-line-args-left (list directory "notes" source destination))) (berlin-ox-hugo-command nil))
            (with-temp-buffer
              (insert-file-contents (expand-file-name "content/notes/source.md" directory))
              (let ((markdown (buffer-string)))
                (should (string-match-p (regexp-quote "id:target-id#stable-detail") markdown))
                (should (string-match-p (regexp-quote "id:target-id)") markdown))
                (should (string-match-p "/attachments/" markdown))
                (should-not (string-match-p "PRIVATE_\\|private-heading\\|attachment:\\|missing-private" markdown))
                (should-not (string-match-p (regexp-quote directory) markdown)))))
          (should (= 1 (length (directory-files-recursively (expand-file-name "static/attachments" directory) "\\.svg\\'"))))
          (should-not (directory-files-recursively (expand-file-name "static/attachments" directory) "unreferenced")))
      (dolist (file (list source target renamed)) (when-let* ((buffer (get-file-buffer file))) (kill-buffer buffer)))
      (delete-directory directory t))))

(ert-deftest berlin-id-based-attachment-layout ()
  (let* ((directory (make-temp-file "berlin-id-attachment-" t))
         (source (expand-file-name "source.org" directory))
         (org-attach-id-dir (expand-file-name "attachments" directory))
         (org-id-locations nil))
    (unwind-protect
        (progn
          (with-temp-file source
            (insert ":PROPERTIES:\n:ID: 12345678-1234-1234-1234-123456789abc\n:END:\n#+TITLE: Attached\n\n[[attachment:result.txt][Download result]]\n"))
          (with-current-buffer (find-file-noselect source)
            (goto-char (point-min))
            (let ((path (org-attach-dir t)))
              (with-temp-file (expand-file-name "result.txt" path) (insert "SAVED_BYTES"))))
          (let ((command-line-args-left (list directory "notes" source))) (berlin-ox-hugo-command nil))
          (let ((files (directory-files-recursively (expand-file-name "static/attachments" directory) "result.txt")))
            (should (= 1 (length files)))
            (with-temp-buffer (insert-file-contents (car files)) (should (equal "SAVED_BYTES" (buffer-string))))))
      (when-let* ((buffer (get-file-buffer source))) (kill-buffer buffer))
      (delete-directory directory t))))
