;;; diagram-integration.el --- Opt-in real TeX acceptance -*- lexical-binding: t; -*-

;; Run only in the diagrams shell. Uses synthetic data in temporary directories.
(require 'ert)
(require 'cl-lib)
(defconst berlin-diagram-repository
  (expand-file-name "../.." (file-name-directory load-file-name)))
(load (expand-file-name "berlin-diagrams.el" (file-name-directory load-file-name)))
(load (expand-file-name "support/ox-hugo/export.el" berlin-diagram-repository))

(defun berlin-diagram-test-project (fixture)
  "Copy FIXTURE and the notebook example into a retained temporary project."
  (let ((project (make-temp-file "berlin-diagram-check-" t)))
    (message "Diagram acceptance project (%s): %s" fixture project)
    (copy-directory (expand-file-name "themes/notebook/example" berlin-diagram-repository)
                    project nil t t)
    (copy-file (expand-file-name (concat "support/fixtures/diagrams/" fixture)
                                berlin-diagram-repository)
               (expand-file-name "diagram.org" project))
    project))

(defun berlin-diagram-test-render-blocks (expected-count)
  "Render exactly EXPECTED-COUNT blocks in the copied synthetic buffer."
  (goto-char (point-min))
  (let ((count 0))
    ;; Only this test's copied fixtures receive automatic trust.
    (cl-letf (((symbol-function 'yes-or-no-p) (lambda (_) t)))
      (while (re-search-forward "^#\\+begin_src latex" nil t)
        (berlin-render-diagram)
        (setq count (1+ count))))
    (should (= count expected-count)))
  (should (buffer-modified-p))
  (should (= expected-count (how-many "^#\\+RESULTS:" (point-min) (point-max))))
  (save-buffer))

(defun berlin-diagram-test-failed-render (image hash)
  "Verify an invalid recompilation preserves IMAGE with HASH and saved source."
  (goto-char (point-min))
  (search-forward "\\begin{tikzpicture}")
  (beginning-of-line)
  (insert "\\BerlinIntentionallyUndefinedCommand\n")
  (search-backward "#+begin_src latex")
  (cl-letf (((symbol-function 'yes-or-no-p) (lambda (_) t)))
    (should-error (berlin-render-diagram)))
  (should (equal hash (berlin-file-sha256 image)))
  (revert-buffer t t))

(defun berlin-diagram-test-export (project source)
  "Export SOURCE into PROJECT while forbidding authoring execution."
  (let ((command-line-args-left (list project "notes" source)))
    (cl-letf (((symbol-function 'org-babel-execute-src-block)
               (lambda (&rest _) (ert-fail "Publishing executed Babel")))
              ((symbol-function 'berlin-render-diagram)
               (lambda () (ert-fail "Publishing compiled a diagram"))))
      (berlin-ox-hugo-command nil)))
  (with-temp-buffer
    (insert-file-contents (expand-file-name "content/notes/diagram.md" project))
    (should-not (string-match-p "tikzpicture\\|Diagram source\\|begin_src" (buffer-string)))))

(defun berlin-diagram-test-build (project binary)
  "Build PROJECT through the real BINARY and notebook theme."
  (with-temp-file (expand-file-name "berlin.pipeline.rhai" project)
    (insert (format
             "pipeline site {
  let website = assemble_website(parse_markdown(load_markdown(\"content/notes/*.md\")), parse_feed(load_data(\"data/feed.csv\")));
  output render_website(website, \"_site\", website_config(#{ theme: %S, url: \"http://localhost:8093\", title: \"Diagram specimen\", author: \"Example Author\", description: \"Synthetic diagram acceptance\" }));
  output copy_assets(load_assets(\"static/attachments/**/*\"), \"_site/attachments\");
}" (expand-file-name "themes/notebook" berlin-diagram-repository))))
  (let ((default-directory project)
        (process-environment (copy-sequence process-environment)))
    (setenv "BERLIN_DIR" project)
    (with-temp-buffer
      (let ((status (call-process binary nil t nil "build" "--pipeline" "site")))
        (should (equal (list status (and (not (equal status 0)) (buffer-string))) '(0 nil)))))))

(defun berlin-diagram-test-published-images (project slug images hashes)
  "Check IMAGES, HASHES and their descriptions in PROJECT's published SLUG."
  (dolist (image images)
    (let* ((name (car image))
           (hash (cdr (assoc name hashes)))
           (relative (concat "attachments/" hash "/" name)))
      (dolist (prefix '("static/" "_site/"))
        (should (equal hash (berlin-file-sha256 (expand-file-name (concat prefix relative) project)))))
      (with-temp-buffer
        (insert-file-contents (expand-file-name (concat "_site/notes/" slug ".html") project))
        (should (search-forward (concat "/" relative) nil t))
        (goto-char (point-min))
        (should (search-forward (concat "alt=\"" (cdr image) "\"") nil t))))))

(defun berlin-diagram-test-round-trip (fixture slug images)
  "Compile FIXTURE, test failure recovery, and publish IMAGES at SLUG."
  (let* ((binary (expand-file-name "target/debug/bln" berlin-diagram-repository))
         (berlin-executable binary)
         (_ (unless (file-executable-p binary)
              (error "Build bln in the default shell before running diagram acceptance")))
         (project (berlin-diagram-test-project fixture))
         (source (expand-file-name "diagram.org" project)))
    (unwind-protect
        (with-current-buffer (find-file-noselect source)
          (berlin-diagram-test-render-blocks (length images))
          (let ((hashes (mapcar
                         (lambda (image)
                           (cons (car image)
                                 (berlin-file-sha256 (expand-file-name (car image) project))))
                         images)))
            (berlin-diagram-test-failed-render
             (expand-file-name (caar hashes) project) (cdar hashes))
            (berlin-diagram-test-export project source)
            (berlin-diagram-test-build project binary)
            (berlin-diagram-test-published-images project slug images hashes)))
      (when-let* ((buffer (get-file-buffer source)))
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(ert-deftest berlin-diagram-compiles-and-publishes-without-reexecution ()
  (berlin-diagram-test-round-trip
   "article.org" "saved-tikz-diagram"
   '(("pipeline.svg" . "Three stages connected by arrows: Org, Documents, HTML."))))

(ert-deftest berlin-complex-diagrams-compile-and-publish-without-reexecution ()
  (berlin-diagram-test-round-trip
   "gallery.org" "tikz-diagram-gallery"
   '(("architecture.svg" . "Nested architecture: Org and Markdown enter Parse, then Transform, which branches to HTML and a text draft.")
     ("states.svg" . "State machine: Ready starts Running; Running can wait, fail into Failed, or finish in Done; Failed retries Ready.")
     ("mathematics.svg" . "Mathematical plot: y equals x squared, tangent y equals two x minus one at (1,1), with area one third shaded under the curve between zero and one."))))

;;; diagram-integration.el ends here
