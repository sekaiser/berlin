;;; berlin-diagrams.el --- Explicit Org diagram authoring -*- lexical-binding: t; -*-

;; Separate from berlin-check and the batch exporter. Loading this file does
;; not execute blocks, change the user's Babel settings, or enable export evaluation.

(require 'org)
(require 'ob-latex)
(require 'ox-latex)
(require 'cl-lib)

(defvar berlin-executable "bln"
  "Berlin executable used by the editor adapters.")

(defun berlin-execute-latex-graphic (body params)
  "Render BODY with PARAMS through Berlin; called only after author consent.
Org still expands variables/noweb and inserts the ordinary saved result link."
  (let* ((file (expand-file-name (cdr (assq :file params))))
         (engine (or (cdr (assq :berlin-engine params)) "pdflatex"))
         (inputs (cdr (assq :berlin-inputs params)))
         (headers (cdr (assq :headers params)))
         (org-latex-packages-alist (append (cdr (assq :packages params)) org-latex-packages-alist))
         (origin default-directory)
         (log (get-buffer-create "*Berlin LaTeX Output*")))
    (unless (member engine '("pdflatex" "lualatex")) (user-error "Unsupported diagram engine: %s" engine))
    (unless (and (listp headers) (cl-every #'stringp headers)) (user-error ":headers must evaluate to a list of TeX strings"))
    (when (and inputs (or (not (stringp inputs)) (file-remote-p inputs))) (user-error "Diagram inputs must name a local directory"))
    (let ((source (make-temp-file "berlin-diagram-" nil ".tex")))
      (unwind-protect
          (progn
            (with-temp-file source
              (insert (funcall org-babel-latex-preamble params) "\n"
                      (mapconcat #'identity headers "\n") "\n"
                      (funcall org-babel-latex-begin-env params) "\n"
                      (org-babel-expand-body:latex body params) "\n"
                      (funcall org-babel-latex-end-env params) "\n"))
            (with-current-buffer log (erase-buffer))
            (let ((status (apply #'call-process berlin-executable nil log nil
                                 (append (list "render-diagram" source "--output" file "--engine" engine
                                               "--cache-root" origin "--allow-tex")
                                         (when inputs (list "--input-root" (expand-file-name inputs origin)))))))
              (unless (equal status 0)
                (error "Diagram rendering failed; previous image retained. See *Berlin LaTeX Output*: %s"
                       (with-current-buffer log (buffer-string))))))
        (delete-file source))))
  nil)

;;;###autoload
(defun berlin-render-diagram ()
  "Run the trusted LaTeX block at point and insert its saved SVG result.
Uses Org Babel and Berlin's shared graphics service. Confirm trust before evaluating any
header arguments. The Org buffer is modified but never saved automatically.
This is author-time code execution, not a sandbox or a publishing operation."
  (interactive)
  (unless (and (derived-mode-p 'org-mode) buffer-file-name
               (not (file-remote-p buffer-file-name)))
    (user-error "Open a local Org file to render a diagram"))
  (let ((block (org-element-context)))
    (unless (and (eq (org-element-type block) 'src-block)
                 (equal (org-element-property :language block) "latex"))
      (user-error "Place point inside a latex source block")))
  (dolist (program (list berlin-executable))
    (unless (executable-find program)
      (user-error "Missing %s; build Berlin and configure berlin-executable" program)))
  (unless (yes-or-no-p "Execute this trusted LaTeX block and its Babel headers? ")
    (user-error "Diagram rendering cancelled"))
  (let* ((default-directory (file-name-directory buffer-file-name))
         (info (org-babel-get-src-block-info))
         (parameters (nth 2 info))
         (file (cdr (assq :file parameters))))
    (when (member (cdr (assq :eval parameters)) '("never" "no"))
      (user-error "This block forbids execution; use :eval never-export for author-time rendering"))
    (unless (and (stringp file) (not (file-remote-p file))
                 (string-suffix-p ".svg" file))
      (user-error "Diagram block requires :file with a local .svg destination"))
    (when (assq :dir parameters)
      (user-error "Diagram paths are relative to the Org file; omit :dir"))
    (unless (file-directory-p (file-name-directory (expand-file-name file)))
      (user-error "Create the diagram destination directory first"))
    (let ((org-confirm-babel-evaluate nil)) ; Trust was confirmed before header evaluation.
      (cl-letf (((symbol-function 'org-babel-execute:latex) #'berlin-execute-latex-graphic))
        (org-babel-execute-src-block nil info)))
    (unless (file-regular-p file)
      (user-error "Org did not produce the requested SVG: %s" file)))
  (message "Diagram rendered. Review the result and save the Org buffer before publishing."))

(provide 'berlin-diagrams)
;;; berlin-diagrams.el ends here
