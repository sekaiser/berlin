;;; code-results.el --- Publish stored Babel results without evaluation -*- lexical-binding: t; -*-

(require 'ob-core)

(defun berlin-code-result-range (block)
  "Return stored result bounds for BLOCK without insertion or evaluation."
  (save-excursion
    (goto-char (org-element-property :post-affiliated block))
    (when-let* ((begin (org-babel-where-is-src-block-result)))
      (goto-char begin)
      (forward-line)
      (cons begin (org-babel-result-end)))))

(defun berlin-code-result-element (tree range)
  "Find the parsed result container in TREE for RANGE.
Trailing blank lines belong to the element, not to Babel's result bounds.
An affiliated RESULTS paragraph is also a container for plain-text results."
  (when range
    (org-element-map tree org-element-all-elements
      (lambda (element)
        (let ((start (or (org-element-property :post-affiliated element)
                         (org-element-property :begin element))))
          (when (and start (>= start (car range))
                     (or (< start (cdr range))
                         (and (org-element-property :results element)
                              (<= (org-element-property :begin element) (car range)))))
            element))) nil t)))

(defun berlin-export-code-results (tree backend _info)
  "Apply literal :exports choices to TREE, using only saved results.
Org's no-evaluation reader preserves inherited and language-specific headers
without resolving variables, sessions, noweb, post-processing or Lisp."
  (when (org-export-derived-backend-p backend 'hugo)
    (let (remove)
      (org-element-map tree 'src-block
        (lambda (block)
          ;; A source block can itself be a stored result, owned by its producer.
          (unless (org-element-property :results block)
            (let* ((headers (nth 2 (org-babel-get-src-block-info t block)))
                   (mode (or (cdr (assq :exports headers)) "code"))
                   (result (berlin-code-result-element tree (berlin-code-result-range block))))
              (unless (member mode '("code" "results" "both" "none"))
                (error "Berlin requires literal :exports code/results/both/none, got %S" mode))
              (when (and result (equal mode "results"))
                (dolist (property '(:name :caption :attr_html))
                  (unless (org-element-property property result)
                    (org-element-put-property result property (org-element-property property block)))))
              (when (and result (member mode '("code" "none"))) (push result remove))
              (when (member mode '("results" "none")) (push block remove))))))
      (dolist (element remove) (org-element-extract-element element))))
  tree)

(provide 'berlin-code-results)
;;; code-results.el ends here
