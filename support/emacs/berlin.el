;;; berlin.el --- Local authoring checks for Berlin -*- lexical-binding: t; -*-

;; Load this file, set `berlin-executable', then run M-x berlin-check in a
;; publishing project. No Org export, Babel execution or configuration edits.

(require 'button)
(require 'json)
(require 'org)
(require 'url-parse)
(require 'url-util)
(require 'subr-x)

(defgroup berlin nil "Berlin authoring reports." :group 'tools)
(defcustom berlin-executable "bln"
  "Berlin executable, either on PATH or an absolute filename."
  :type 'string :group 'berlin)
(defcustom berlin-check-pipeline "site"
  "Website pipeline inspected by `berlin-check'."
  :type 'string :group 'berlin)
(defvar-local berlin-check--project nil)
(defvar-local berlin-check--pipeline nil)
(defvar-local berlin-check--process nil)

(define-derived-mode berlin-check-mode special-mode "Berlin"
  "Read-only findings. RET follows a source; n/p move between sources."
  (setq-local revert-buffer-function #'berlin-check--refresh))
(define-key berlin-check-mode-map (kbd "n") #'forward-button)
(define-key berlin-check-mode-map (kbd "p") #'backward-button)
(define-key berlin-check-mode-map (kbd "g") #'revert-buffer)

(defun berlin-check--refresh (&rest _)
  (berlin-check berlin-check--project berlin-check--pipeline))

;;;###autoload
(defun berlin-check (project &optional pipeline)
  "Inspect PROJECT's existing Markdown using PIPELINE.
With a prefix argument, prompt for the pipeline. Never save buffers or export
Org. Source changes must be explicitly saved and exported beforehand."
  (interactive
   (list (read-directory-name "Berlin project: "
                              (or (locate-dominating-file default-directory "berlin.pipeline.rhai")
                                  default-directory))
         (if current-prefix-arg (read-string "Pipeline: " berlin-check-pipeline)
           berlin-check-pipeline)))
  (setq project (file-name-as-directory (expand-file-name project))
        pipeline (or pipeline berlin-check-pipeline))
  (when (file-remote-p project) (user-error "Berlin checks require a local project"))
  (let ((report (get-buffer-create (format "*Berlin: %s*" (abbreviate-file-name project)))))
    (with-current-buffer report
      (when (process-live-p berlin-check--process) (user-error "A Berlin check is already running"))
      (berlin-check-mode)
      (setq berlin-check--project project berlin-check--pipeline pipeline)
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert "Checking existing Markdown…\nNo buffers are saved or exported.\n")))
    (pop-to-buffer report)
    (let ((stdout (generate-new-buffer " *Berlin JSON*"))
          (stderr (generate-new-buffer " *Berlin stderr*"))
          (default-directory project)
          (process-environment (copy-sequence process-environment)))
      (setenv "BERLIN_DIR" project)
      (condition-case error
          (with-current-buffer report
            (setq berlin-check--process
                  (make-process
                   :name "berlin-check" :buffer stdout :stderr stderr
                   :connection-type 'pipe :coding 'utf-8-unix :noquery t
                   :command (list berlin-executable "check" "--pipeline" pipeline "--json")
                   :sentinel
                   (lambda (process _event)
                     (when (memq (process-status process) '(exit signal))
                       (unwind-protect
                           (when (buffer-live-p report)
                             (with-current-buffer report
                               (setq berlin-check--process nil)
                               (condition-case error
                                   (let ((data (with-current-buffer stdout
                                                 (goto-char (point-min))
                                                 (json-parse-buffer :object-type 'alist :array-type 'list
                                                                    :null-object nil :false-object nil))))
                                     (berlin-check--render data (process-exit-status process)))
                                 (error (berlin-check--failure
                                         (format "%s\n%s" (error-message-string error)
                                                 (with-current-buffer stderr (buffer-string))))))))
                         (kill-buffer stdout)
                         (kill-buffer stderr)))))))
        (error
         (kill-buffer stdout)
         (kill-buffer stderr)
         (with-current-buffer report (berlin-check--failure (error-message-string error))))))))

(defun berlin-check--failure (message)
  "Show MESSAGE without implying that analysis completed."
  (let ((inhibit-read-only t))
    (erase-buffer)
    (insert "Berlin check incomplete\n\n" message "\n")))

(defun berlin-check--render (data exit-code)
  "Display versioned DATA, including findings when EXIT-CODE is nonzero."
  (unless (= (or (alist-get 'schema_version data) 0) 1)
    (error "Unsupported Berlin report schema"))
  (if (equal (alist-get 'status data) "incomplete")
      (berlin-check--failure (alist-get 'message data))
    (unless (and (equal (alist-get 'status data) "complete") (memq exit-code '(0 1)))
      (error "Berlin did not complete normally"))
    (let* ((report (alist-get 'report data))
           (findings (alist-get 'findings report))
           (inhibit-read-only t))
      (erase-buffer)
      (insert (format "Berlin · %s\nPublished documents: %s; guides: %s; excluded drafts: %s\n\n"
                      (alist-get 'pipeline data) (alist-get 'published_documents report)
                      (alist-get 'guides report) (alist-get 'excluded_drafts report)))
      (insert "Existing export only. Observations are optional. g: check again; RET: visit source.\n\n")
      (if findings
          (dolist (finding findings)
            (let* ((location (alist-get 'location finding))
                   (origin (alist-get 'origin location))
                   (source (or (alist-get 'source origin) (alist-get 'source location)))
                   (heading (alist-get 'heading origin)))
              (insert (format "%s · %s · %s\n" (alist-get 'severity finding)
                              (alist-get 'code finding) (alist-get 'document location)))
              (when-let* ((target (alist-get 'target finding))) (insert "  Target: " target "\n"))
              (insert "  ")
              (let ((project berlin-check--project))
                (insert-text-button source 'follow-link t
                                    'action (lambda (_) (berlin-check--visit project location))))
              (when heading (insert " — " (string-join (alist-get 'outline heading) " / ")))
              (when (alist-get 'stale origin) (insert " (Org changed since export; file only)"))
              (unless origin (insert " (Markdown; no matching Org origin)"))
              (insert "\n")
              (when-let* ((excerpt (alist-get 'excerpt location))) (insert "  " excerpt "\n"))
              (insert "\n")))
        (insert "No findings.\n"))
      (goto-char (point-min)))))

(defun berlin-check--local-file (uri project)
  "Decode URI only when it names an existing local file inside PROJECT."
  (let* ((url (url-generic-parse-url uri))
         (path (decode-coding-string (url-unhex-string (url-filename url)) 'utf-8)))
    (unless (and (equal (url-type url) "file")
                 (member (url-host url) '(nil "" "localhost"))
                 (not (file-remote-p path)) (file-regular-p path)
                 (file-in-directory-p path project))
      (user-error "Source is missing or outside this local project"))
    path))

(defun berlin-check--file-hash (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (secure-hash 'sha256 (current-buffer))))

(defun berlin-check--heading-position (heading)
  "Find HEADING in this file only; ambiguous IDs or outline paths return nil."
  (let ((id (alist-get 'id heading))
        (outline (alist-get 'outline heading))
        positions)
    (org-map-entries
     (lambda ()
       (when (if id (equal id (org-entry-get nil "ID"))
               (equal outline (org-get-outline-path t)))
         (push (point) positions))) nil nil)
    (when (= (length positions) 1) (car positions))))

(defun berlin-check--visit (project location)
  "Visit LOCATION without guessing line numbers or changing the source."
  (let* ((origin (alist-get 'origin location))
         (source (or (alist-get 'source origin) (alist-get 'source location)))
         (path (berlin-check--local-file source project)))
    (find-file path)
    (widen)
    (goto-char (point-min))
    (cond
     ((not origin) (message "Markdown source opened; no matching Org origin"))
     ((or (alist-get 'stale origin) (buffer-modified-p)
          (not (verify-visited-file-modtime (current-buffer)))
          (not (equal (alist-get 'source_hash origin) (berlin-check--file-hash path))))
      (message "Org has changed since export; opened the file without guessing a heading"))
     ((alist-get 'heading origin)
      (let ((position (and (derived-mode-p 'org-mode)
                           (berlin-check--heading-position (alist-get 'heading origin)))))
        (if position
            (progn (goto-char position) (org-fold-show-context 'link-search) (org-fold-show-entry))
          (message "Heading is missing or ambiguous; opened the Org file")))))))

(provide 'berlin)
;;; berlin.el ends here
