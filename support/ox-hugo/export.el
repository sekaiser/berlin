;;; export.el --- Berlin batch exporter for Ox-Hugo -*- lexical-binding: t; -*-

(require 'org)
(unless (require 'ox-hugo nil t)
  (error "Ox-Hugo is unavailable; install ox-hugo where batch Emacs can load it"))

(defvar berlin-content-id nil
  "Stable content ID added to the current Ox-Hugo export.")

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
                    (org-hugo-front-matter-format "yaml")
                    (org-hugo-section section))
                (unless berlin-content-id
                  (error "Org source %s has no file-level ID" source))
                (org-hugo-export-to-md))
              (kill-buffer))))
      (when (file-exists-p org-id-locations-file)
        (delete-file org-id-locations-file)))))

(add-to-list 'command-switch-alist
             '("--berlin-export-org" . berlin-ox-hugo-command))

;;; export.el ends here
