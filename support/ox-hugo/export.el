;;; export.el --- Berlin batch exporter for Ox-Hugo -*- lexical-binding: t; -*-

(require 'org)
(require 'cl-lib)
(require 'json)
(unless (require 'ox-hugo nil t)
  (error "Ox-Hugo is unavailable; install ox-hugo where batch Emacs can load it"))

(defvar berlin-content-id nil
  "Stable content ID added to the current Ox-Hugo export.")

(defvar berlin-source-headings nil
  "Local heading origins collected during one export, never embedded in Markdown.")

(defun berlin-file-sha256 (file)
  "Hash the exact bytes of FILE, independently of buffer coding conventions."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun berlin-heading-origin (heading)
  "Describe HEADING using an optional Org ID and its original outline path."
  (let ((current heading) outline)
    (while current
      (push (org-element-property :raw-value current) outline)
      (setq current (org-export-get-parent-headline current)))
    `((id . ,(org-element-property :ID heading))
      (outline . ,(vconcat outline)))))

(defun berlin-export-with-origins ()
  "Export this buffer and return local provenance for the resulting Markdown."
  (let ((original (symbol-function 'org-hugo-heading))
        (berlin-source-headings nil)
        (source (buffer-file-name)))
    (cl-letf (((symbol-function 'org-hugo-heading)
               (lambda (heading contents info)
                 (let ((rendered (funcall original heading contents info))
                       (style (plist-get info :md-headline-style))
                       (level (+ (org-export-get-relative-level heading info)
                                 (string-to-number (plist-get info :hugo-level-offset)))))
                   ;; Low-level headings become lists, not semantic sections.
                   (when (and rendered org-hugo-headline-anchor
                              (not (org-export-low-level-p heading info))
                              (or (and (eq style 'atx) (<= level 6))
                                  (and (eq style 'setext) (<= level 2))))
                     (push `((fragment . ,(org-hugo--get-anchor heading info))
                             (heading . ,(berlin-heading-origin heading)))
                           berlin-source-headings))
                   rendered))))
      (let ((markdown (org-hugo-export-to-md)))
        `((document . ,berlin-content-id)
          (source . ,source)
          (source_hash . ,(berlin-file-sha256 source))
          (markdown . ,(expand-file-name markdown))
          (markdown_hash . ,(berlin-file-sha256 markdown))
          (headings . ,(vconcat (nreverse berlin-source-headings))))))))

(defun berlin-ox-hugo-id-link (path description backend info)
  "Preserve the stable target of an Org ID link for Berlin's assembly.
PATH and DESCRIPTION are the authored link; BACKEND and INFO come from Org."
  (when (eq backend 'md)
    (let ((target path)
          (location (org-id-find path 'marker)))
      (when location
        (with-current-buffer (marker-buffer location)
          (save-excursion
            (goto-char (point-min))
            (let ((document-id (org-entry-get nil "ID")))
              (when (and document-id (not (equal path document-id)))
                (goto-char location)
                (setq target (concat document-id (org-hugo--get-anchor-at-point info))))))))
      (format "[%s](id:%s)" (or description path) target))))

(defun berlin-ox-hugo-add-content-id (options backend)
  "Add `berlin-content-id' to Hugo export OPTIONS for BACKEND."
  (when (and berlin-content-id
             (org-export-derived-backend-p backend 'hugo))
    (let ((custom (plist-get options :hugo-custom-front-matter)))
      (plist-put options :hugo-custom-front-matter
                 (if custom
                     (format ":id %S %s" berlin-content-id custom)
                   (format ":id %S" berlin-content-id)))))
  options)

(defun berlin-ox-hugo-command (_switch)
  "Export Org files supplied after SWITCH using project-local conventions."
  (let* ((base-dir (expand-file-name (pop command-line-args-left)))
         (section (pop command-line-args-left))
         (sources command-line-args-left)
         (origins-file (getenv "BERLIN_ORG_SOURCE_MAP"))
         origins
         (org-id-locations-file
          (make-temp-file "berlin-org-id-locations-")))
    (setq command-line-args-left nil)
    (unless (and base-dir section sources)
      (error "Usage: --berlin-export-org BASE-DIR SECTION SOURCE..."))
    (delete-file org-id-locations-file)
    (unwind-protect
        (progn
          (org-id-update-id-locations sources t)
          (dolist (source sources)
            (with-current-buffer (find-file-noselect source)
              (goto-char (point-min))
              (let ((berlin-content-id (org-entry-get nil "ID"))
                    ;; Publishing reads source and stored results; it must not run examples.
                    (org-export-use-babel nil)
                    (org-export-filter-options-functions
                     (cons #'berlin-ox-hugo-add-content-id
                           org-export-filter-options-functions))
                    (org-hugo-base-dir base-dir)
                    (org-link-parameters (copy-tree org-link-parameters))
                    (org-hugo-front-matter-format "yaml")
                    (org-hugo-section section))
                (unless berlin-content-id
                  (error "Org source %s has no file-level ID" source))
                (org-link-set-parameters "id" :export #'berlin-ox-hugo-id-link)
                (push (berlin-export-with-origins) origins))
              (kill-buffer)))
          (when origins-file
            (let ((coding-system-for-write 'utf-8-unix))
              (with-temp-file origins-file
                (insert (json-encode `((schema_version . 1)
                                       (documents . ,(vconcat (nreverse origins))))))))))
      (when (file-exists-p org-id-locations-file)
        (delete-file org-id-locations-file)))))

(add-to-list 'command-switch-alist
             '("--berlin-export-org" . berlin-ox-hugo-command))

;;; export.el ends here
