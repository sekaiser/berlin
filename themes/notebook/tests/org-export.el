;;; org-export.el --- Theme authoring contracts -*- lexical-binding: t; -*-

(require 'ert)
(defconst notebook-test-theme
  (expand-file-name ".." (file-name-directory load-file-name)))

(defun notebook-test-export (name slug check &optional assets)
  "Export authoring fixture NAME, verify every figure, and call CHECK.
CHECK receives Markdown and, when BERLIN_TEST_BINARY is set, published HTML.
ASSETS maps theme-relative fixture paths to local source filenames."
  (let* ((directory (make-temp-file "berlin-theme-export-" t))
         (source (expand-file-name "source.org" directory))
         (fixture (expand-file-name (concat "example/authoring/" name ".org") notebook-test-theme))
         (figures (with-temp-buffer
                    (insert-file-contents fixture)
                    (let (figures)
                      (while (re-search-forward "<figure\\(?:.\\|\n\\)*?</figure>" nil t)
                        (push (match-string-no-properties 0) figures))
                      (nreverse figures))))
         markdown html)
    (unwind-protect
        (progn
          (copy-file fixture source)
          (dolist (asset assets)
            (copy-file (expand-file-name (car asset) notebook-test-theme)
                       (expand-file-name (cdr asset) directory)))
          ;; Presentation must never cause an example to execute during export.
          (cl-letf (((symbol-function 'org-babel-execute-src-block)
                     (lambda (&rest _) (error "Theme export executed Babel"))))
            (let ((command-line-args-left (list directory "notes" source)))
              (berlin-ox-hugo-command nil)))
          (setq markdown (with-temp-buffer
                           (insert-file-contents (expand-file-name "content/notes/source.md" directory))
                           (buffer-string)))
          ;; Raw-HTML fixtures preserve their figures verbatim. Native Org image
          ;; fixtures are checked after attachment URLs and captions are rendered.
          (dolist (figure figures) (should (string-match-p (regexp-quote figure) markdown)))
          (when-let* ((binary (getenv "BERLIN_TEST_BINARY")))
            (with-temp-file (expand-file-name "berlin.pipeline.rhai" directory)
              (insert-file-contents (expand-file-name "../../cli/tests/support/notebook.rhai" notebook-test-theme))
              (goto-char (point-max))
              (insert "\npipeline test {\n"
                      "let sources=catalog([documents(load_markdown(\"content/notes/*.md\"))],publication_options(#{source_site_url:\"https://notebook.example\"}));\n"
                      "output render_html(compose(sources,Fn(\"notebook\")),\"_site\",html_config(#{theme: "
                      (prin1-to-string notebook-test-theme)
                      ", url: \"https://notebook.example\", title: \"Example\", author: \"Example Author\", description: \"Synthetic theme specimen\"}));\n}\n"))
            (let ((default-directory directory) (process-environment (copy-sequence process-environment)))
              (setenv "BERLIN_DIR" directory)
              (with-temp-buffer
                (let ((status (call-process binary nil t nil "build" "--pipeline" "test")))
                  (should (equal (list status (unless (equal status 0) (buffer-string))) '(0 nil))))))
            (setq html (with-temp-buffer
                         (insert-file-contents (expand-file-name (concat "_site/notes/" slug ".html") directory))
                         (buffer-string)))
            (dolist (figure figures) (should (string-match-p (regexp-quote figure) html)))
            (dolist (asset assets)
              (let* ((hash (berlin-file-sha256 (expand-file-name (car asset) notebook-test-theme)))
                     (published (expand-file-name (concat "_site/attachments/" hash "/" (cdr asset)) directory)))
                (should (equal hash (berlin-file-sha256 published))))))
          (funcall check markdown html))
      (when-let* ((buffer (get-file-buffer source))) (kill-buffer buffer))
      (delete-directory directory t))))

(ert-deftest notebook-traversal-survives-org-export ()
  (notebook-test-export
   "traversal" "following-a-value"
   (lambda (markdown html)
     (should (string-match-p (regexp-quote "<a id=\"code-snippet--traversal-example\"></a>") markdown))
     (should (string-match-p (regexp-quote "println!(\"{message}\");") markdown))
     (when html
       (dolist (line '(2 3))
         (should (string-match-p (format "id=\"code-snippet--traversal-example-L%d\"" line) html)))))))

(ert-deftest notebook-results-survive-org-export ()
  (notebook-test-export
   "results" "result-tables"
   (lambda (markdown html)
     (dolist (output (delq nil (list markdown html)))
       (should (string-match-p (regexp-quote "accepted=12\ndeferred=3\nrejected=0") output))
       (should (string-match-p "class=\"notebook-result notebook-result--forest\"" output))
       (should (string-match-p "class=\"notebook-result\"" output))))))

(ert-deftest notebook-comparison-survives-org-export ()
  (notebook-test-export
   "comparison" "comparing-representations"
   (lambda (markdown html)
     (dolist (output (delq nil (list markdown html)))
       (should (string-match-p "class=\"notebook-comparison\"" output))
       (should (string-match-p (regexp-quote "\"name\": \"Example\"") output))
       (should (string-match-p (regexp-quote "{\"name\":\"Example\",\"enabled\":true") output))))))

(ert-deftest notebook-diagram-keeps-native-org-attachments ()
  (notebook-test-export
   "diagrams" "diagram-plates"
   (lambda (markdown html)
     (let* ((asset (expand-file-name "example/static/diagrams/notebook-plot.svg" notebook-test-theme))
            (hash (berlin-file-sha256 asset))
            (url (concat "/attachments/" hash "/notebook-plot.svg")))
       (dolist (output (delq nil (list markdown html)))
         (should (string-match-p "class=\"notebook-diagram\"" output))
         (should (string-match-p "class=\"diagram-legend\"" output))
         (should (string-match-p (regexp-quote url) output)))
       (when html
         (should (string-match-p (regexp-quote (concat "href=\"" url "\"")) html))
         (should (string-match-p "alt=\"Line plot connecting the synthetic points" html))
         (should (string-match-p "<figcaption>.*Four points from the synthetic graphics fixture" html)))))
   '(("example/static/diagrams/notebook-plot.svg" . "notebook-plot.svg"))))

(ert-deftest notebook-observation-survives-native-org-export ()
  (notebook-test-export
   "observations" "authored-observations"
   (lambda (markdown html)
     (dolist (output (delq nil (list markdown html)))
       (should (string-match-p "class=\"notebook-observation\"" output))
       (should (string-match-p "role=\"note\"" output))
       (should (string-match-p "aria-label=\"Zero is still a value\"" output))
       (should (string-match-p "class=\"observation-title\"" output))
       (should (string-match-p "Some(0)" output)))
     (when html
       (should (string-match-p "<code>None</code>" html))
       (should (string-match-p "href=\"#code-snippet--observation-example-L2\"" html))
       (should (string-match-p "id=\"code-snippet--observation-example-L2\"" html))))))
