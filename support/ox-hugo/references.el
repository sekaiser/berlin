;;; references.el --- Native Org citations for Berlin -*- lexical-binding: t; -*-

(require 'oc)
(require 'bibtex)
(require 'cl-lib)
(require 'subr-x)
(require 'json)

(defvar berlin-export-references nil
  "Bibliography snapshots for one export, serialized as YAML-compatible JSON.")

(defun berlin-bibliography-field (name entry)
  "Read literal NAME from BibTeX ENTRY without interpreting TeX code."
  (when-let* ((value (cdr (assoc-string name entry t))))
    ;; This authoring profile is intentionally plain text. Rich TeX bibliographies
    ;; belong to the paper compiler, which uses BibTeX's actual rendered output.
    (when (string-match-p "[\\\\#]" value)
      (error "Org bibliography field %s requires literal text, not TeX or string expressions" name))
    (string-trim (replace-regexp-in-string "[{}]" "" value))))

(defun berlin-bibliography-entries (files)
  "Read literal bibliographic records from local FILES for Markdown front matter."
  (let (entries keys)
    (dolist (file files)
      (when (or (file-remote-p file) (not (file-regular-p file)))
        (error "Bibliography must be a readable local file: %s" file))
      (when (> (file-attribute-size (file-attributes file)) (* 4 1024 1024))
        (error "Org bibliography exceeds 4 MiB: %s" file))
      (with-temp-buffer
        (delay-mode-hooks (bibtex-mode))
        (insert-file-contents file)
        (goto-char (point-max))
        (unless (bolp) (insert "\n"))
        (goto-char (point-min))
        (bibtex-set-dialect 'BibTeX t)
        (unless (bibtex-validate) (error "Invalid BibTeX bibliography: %s" file))
        (bibtex-map-entries
         (lambda (_key _start _end)
           (let* ((raw (save-excursion (bibtex-parse-entry)))
                  (entry (bibtex-parse-entry t))
                  (key (cdr (assoc "=key=" entry)))
                  (title (berlin-bibliography-field "title" entry))
                  (url (berlin-bibliography-field "url" entry)))
             (dolist (field raw)
               (unless (or (string-prefix-p "=" (car field))
                           (string-prefix-p "{" (cdr field))
                           (string-prefix-p "\"" (cdr field))
                           (string-match-p "\\`[0-9]+\\'" (cdr field)))
                 (error "Org bibliography values must be literal: %s" (car field))))
             (unless (and key (not (member key keys)) title (not (string-empty-p title)))
               (error "Bibliography needs distinct keys and titles: %s" key))
             (dolist (field '("crossref" "xdata"))
               (when (assoc-string field entry t)
                 (error "Org bibliography inheritance is unsupported: %s" field)))
             (push key keys)
             (push `((id . ,key)
                     (text . ,(string-join
                               (delq nil
                                     (mapcar (lambda (field) (berlin-bibliography-field field entry))
                                             '("author" "title" "journal" "publisher" "year" "note")))
                               ". "))
                     ,@(when url `((url . ,url)))) entries))))))
    (nreverse entries)))

(defun berlin-ox-hugo-add-references (options backend)
  "Carry declared bibliographies into Hugo export OPTIONS for BACKEND."
  (when (org-export-derived-backend-p backend 'hugo)
    (when-let* ((files (org-cite-list-bibliography-files)))
      (setq berlin-export-references (berlin-bibliography-entries files))))
  options)

(defun berlin-ox-hugo-write-references (output backend _info)
  "Add structured references to OUTPUT after ox-hugo has serialized its metadata.
The legacy YAML serializer cannot represent arrays of records. JSON is a YAML
subset, so no second general-purpose serializer is necessary here."
  (if (and berlin-export-references (org-export-derived-backend-p backend 'hugo))
      (progn
        (unless (string-prefix-p "---\n" output)
          (error "Berlin citations require YAML front matter"))
        (let ((end (string-match "\n---\n" output 4)))
          (unless end (error "Missing YAML front matter delimiter"))
          (when (string-match-p "\nreferences:" (substring output 0 end))
            (error "Do not combine an Org bibliography with manual references metadata")))
        (concat "---\nreferences: " (json-encode (vconcat berlin-export-references)) "\n"
                (substring output 4)))
    output))

(defun berlin-export-citation (citation style _backend info)
  "Export native CITATION to Berlin's linked numeric references.
Preserve authored prefixes and locators; do not approximate other citation styles."
  (unless (member style '((nil . nil) ("default" . nil)))
    (error "Berlin Org citations currently support the default numeric style"))
  (let ((render (lambda (object property)
                  (org-export-data (org-element-property property object) info))))
    (concat
     (funcall render citation :prefix)
     (mapconcat
      (lambda (reference)
        (concat (funcall render reference :prefix)
                "\\cite{" (org-element-property :key reference) "}"
                (funcall render reference :suffix)))
      (org-cite-get-references citation) "; ")
     (funcall render citation :suffix))))

(org-cite-register-processor 'berlin
  :export-citation #'berlin-export-citation
  ;; The semantic document places the bibliography once, after the article.
  :export-bibliography (lambda (&rest _) nil))

(provide 'berlin-references)
;;; references.el ends here
