--[[ @longbow
name = "github"
version = "0.3.0"
description = "GitHub repository, issues, PRs, and contributors"
]]

-- Each endpoint declares its columns next to the `map` that produces them:
-- `datatype` is one of the ten logical types Longbow writes Parquet with
-- (timestamps become real timestamps, not text), and `brightflow` carries the
-- analysis role, KPI flag and display defaults Brightflow files under this
-- connector's name. A person's later edit in Brightflow wins over these; a
-- re-sync only refreshes what this file says.

return function(p)
    -- Config: single "owner/repo" slug
    local repo_slug = p.config.repo or "anthropics/claude-code"
    local owner, repo = repo_slug:match("^([^/]+)/(.+)$")
    assert(owner and repo, "github connector: 'repo' must be 'owner/repo' (got " .. tostring(repo_slug) .. ")")

    -- Cursor values for incremental sync (injected by Brightflow scheduler)
    local cursors = p.config._cursors or {}

    p.base_url("https://api.github.com")

    -- Auth: GitHub expects "token xxx" not "Bearer xxx"
    p.auth(function(ctx)
        ctx.headers["Authorization"] = "token " .. p.config.token
        ctx.headers["Accept"] = "application/vnd.github+json"
        ctx.headers["X-GitHub-Api-Version"] = "2022-11-28"
        ctx.headers["User-Agent"] = "Brightflow/0.1.0"
    end)

    -- Pagination: GitHub uses Link headers
    p.paginate(paginate.link_header())

    -- Rate limiting: GitHub returns these headers
    p.rate_limit(rate_limit.header({
        remaining = "x-ratelimit-remaining",
        reset = "x-ratelimit-reset",
        threshold = 100,
    }))

    -- Retry on failure
    p.retry(retry.exponential({
        max_attempts = 3,
        base_delay = 1000,
    }))

    -- Output to Parquet
    p.output(output.parquet({
        path = p.config.output_path,
    }))

    -- Repository endpoint (single object)
    p.endpoint("repository", {
        path = "/repos/" .. owner .. "/" .. repo,
        primary_key = {"id"},
        description = "The repository itself: one row, refreshed on every sync",
        columns = {
            id = { datatype = "Integer", brightflow = { role = "ignored" } },
            name = { datatype = "String", brightflow = { role = "ignored" } },
            full_name = { datatype = "String", brightflow = { role = "ignored" } },
            description = { datatype = "String", brightflow = { role = "ignored" } },
            owner_login = { datatype = "String", brightflow = { role = "dimension" } },
            owner_id = { datatype = "Integer", brightflow = { role = "ignored" } },
            private = { datatype = "Boolean", brightflow = { role = "dimension" } },
            fork = { datatype = "Boolean", brightflow = { role = "dimension" } },
            created_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            updated_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            pushed_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            stargazers_count = { datatype = "Integer", brightflow = { role = "measure", is_kpi = true, label = "Stars" } },
            watchers_count = { datatype = "Integer", brightflow = { role = "measure", label = "Watchers" } },
            forks_count = { datatype = "Integer", brightflow = { role = "measure", label = "Forks" } },
            open_issues_count = { datatype = "Integer", brightflow = { role = "measure", label = "Open issues" } },
            language = { datatype = "String", brightflow = { role = "dimension" } },
            topics = { datatype = "String", description = "Comma-joined topic list", brightflow = { role = "ignored" } },
            default_branch = { datatype = "String", brightflow = { role = "dimension" } },
            has_issues = { datatype = "Boolean", brightflow = { role = "dimension" } },
            has_projects = { datatype = "Boolean", brightflow = { role = "dimension" } },
            has_wiki = { datatype = "Boolean", brightflow = { role = "dimension" } },
            archived = { datatype = "Boolean", brightflow = { role = "dimension" } },
            disabled = { datatype = "Boolean", brightflow = { role = "dimension" } },
            visibility = { datatype = "String", brightflow = { role = "dimension" } },
            size = { datatype = "Integer", description = "Repository size in kilobytes", brightflow = { role = "measure" } },
            subscribers_count = { datatype = "Integer", brightflow = { role = "measure", label = "Subscribers" } },
            network_count = { datatype = "Integer", brightflow = { role = "measure" } },
        },
        brightflow = {
            display_name = "Repository",
            doc = { id = "id", title = "full_name", body = "description", timestamp = "updated_at" },
        },
        map = function(r)
            return {
                id = r.id,
                name = r.name,
                full_name = r.full_name,
                description = r.description or "",
                owner_login = r.owner and r.owner.login or "",
                owner_id = r.owner and r.owner.id or 0,
                private = r.private,
                fork = r.fork,
                created_at = r.created_at,
                updated_at = r.updated_at,
                pushed_at = r.pushed_at,
                stargazers_count = r.stargazers_count or 0,
                watchers_count = r.watchers_count or 0,
                forks_count = r.forks_count or 0,
                open_issues_count = r.open_issues_count or 0,
                language = r.language or "",
                topics = r.topics and table.concat(r.topics, ",") or "",
                default_branch = r.default_branch or "main",
                has_issues = r.has_issues,
                has_projects = r.has_projects,
                has_wiki = r.has_wiki,
                archived = r.archived,
                disabled = r.disabled,
                visibility = r.visibility or "public",
                size = r.size or 0,
                subscribers_count = r.subscribers_count or 0,
                network_count = r.network_count or 0,
            }
        end,
    })

    -- Issues endpoint (includes both issues and PRs)
    p.endpoint("issues", {
        path = "/repos/" .. owner .. "/" .. repo .. "/issues",
        primary_key = {"id"},
        cursor_field = "updated_at",
        params = {
            state = "all",
            per_page = 100,
            sort = "updated",
            direction = "desc",
            since = cursors.issues,  -- nil if no cursor (fetches all)
        },
        description = "Issues and pull requests of the repository, one row each; "
            .. "`is_pull_request` tells them apart",
        columns = {
            id = { datatype = "Integer", brightflow = { role = "ignored" } },
            number = { datatype = "Integer", brightflow = { role = "ignored" } },
            title = { datatype = "String", brightflow = { role = "ignored" } },
            body = { datatype = "String", brightflow = { role = "ignored" } },
            state = { datatype = "String", description = "open or closed", brightflow = { role = "dimension" } },
            state_reason = { datatype = "String", description = "Why it was closed: completed, not_planned, reopened", brightflow = { role = "dimension" } },
            user_login = { datatype = "String", description = "Who opened it", brightflow = { role = "dimension", label = "Author" } },
            user_id = { datatype = "Integer", brightflow = { role = "ignored" } },
            assignee_logins = { datatype = "String", description = "Comma-joined assignee logins", brightflow = { role = "ignored" } },
            label_names = { datatype = "String", description = "Comma-joined label names", brightflow = { role = "dimension", label = "Labels" } },
            milestone_title = { datatype = "String", brightflow = { role = "ignored" } },
            milestone_number = { datatype = "Integer", brightflow = { role = "ignored" } },
            comments = { datatype = "Integer", description = "Number of comments", brightflow = { role = "measure", is_kpi = true } },
            is_pull_request = { datatype = "Boolean", description = "True for a pull request, false for an issue", brightflow = { role = "dimension" } },
            created_at = { datatype = "DateTimeTz", description = "When it was opened", brightflow = { role = "time", label = "Opened" } },
            updated_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            closed_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            author_association = { datatype = "String", description = "The author's relation to the repository: OWNER, MEMBER, CONTRIBUTOR, NONE", brightflow = { role = "dimension" } },
            locked = { datatype = "Boolean", brightflow = { role = "dimension" } },
            active_lock_reason = { datatype = "String", brightflow = { role = "ignored" } },
            reactions_total = { datatype = "Integer", description = "Total emoji reactions", brightflow = { role = "measure", is_kpi = true, label = "Reactions" } },
            html_url = { datatype = "String", brightflow = { role = "ignored" } },
        },
        brightflow = {
            display_name = "Issues",
            time_granularity = "week",
            comparison_periods = 4,
            doc = { id = "id", number = "number", title = "title", body = "body", timestamp = "created_at", url_template = "{html_url}" },
        },
        map = function(r)
            -- Extract assignee logins
            local assignee_logins = ""
            if r.assignees then
                local logins = {}
                for _, a in ipairs(r.assignees) do
                    table.insert(logins, a.login)
                end
                assignee_logins = table.concat(logins, ",")
            end

            -- Extract label names
            local label_names = ""
            if r.labels then
                local names = {}
                for _, l in ipairs(r.labels) do
                    table.insert(names, l.name)
                end
                label_names = table.concat(names, ",")
            end

            return {
                id = r.id,
                number = r.number,
                title = r.title,
                body = r.body or "",
                state = r.state,
                state_reason = r.state_reason or "",
                user_login = r.user and r.user.login or "",
                user_id = r.user and r.user.id or 0,
                assignee_logins = assignee_logins,
                label_names = label_names,
                milestone_title = r.milestone and r.milestone.title or "",
                milestone_number = r.milestone and r.milestone.number or nil,
                comments = r.comments or 0,
                is_pull_request = r.pull_request ~= nil,
                created_at = r.created_at,
                updated_at = r.updated_at,
                closed_at = r.closed_at,
                author_association = r.author_association or "",
                locked = r.locked,
                active_lock_reason = r.active_lock_reason or "",
                reactions_total = r.reactions and r.reactions.total_count or 0,
                html_url = r.html_url or "",
            }
        end,
    })

    -- Pull Requests endpoint (PR-specific data)
    p.endpoint("pull_requests", {
        path = "/repos/" .. owner .. "/" .. repo .. "/pulls",
        primary_key = {"id"},
        cursor_field = "updated_at",
        params = {
            state = "all",
            per_page = 100,
            sort = "updated",
            direction = "desc",
            since = cursors.pull_requests,  -- nil if no cursor (fetches all)
        },
        description = "Pull requests with review and diff statistics",
        columns = {
            id = { datatype = "Integer", brightflow = { role = "ignored" } },
            number = { datatype = "Integer", brightflow = { role = "ignored" } },
            title = { datatype = "String", brightflow = { role = "ignored" } },
            body = { datatype = "String", brightflow = { role = "ignored" } },
            state = { datatype = "String", description = "open or closed", brightflow = { role = "dimension" } },
            user_login = { datatype = "String", brightflow = { role = "dimension", label = "Author" } },
            user_id = { datatype = "Integer", brightflow = { role = "ignored" } },
            draft = { datatype = "Boolean", brightflow = { role = "dimension" } },
            head_ref = { datatype = "String", brightflow = { role = "ignored" } },
            head_sha = { datatype = "String", brightflow = { role = "ignored" } },
            base_ref = { datatype = "String", description = "Target branch", brightflow = { role = "dimension" } },
            base_sha = { datatype = "String", brightflow = { role = "ignored" } },
            merged = { datatype = "Boolean", brightflow = { role = "dimension" } },
            mergeable = { datatype = "Boolean", brightflow = { role = "dimension" } },
            merged_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            merged_by_login = { datatype = "String", brightflow = { role = "dimension", label = "Merged by" } },
            merge_commit_sha = { datatype = "String", brightflow = { role = "ignored" } },
            commits = { datatype = "Integer", brightflow = { role = "measure" } },
            additions = { datatype = "Integer", description = "Lines added", brightflow = { role = "measure", is_kpi = true } },
            deletions = { datatype = "Integer", description = "Lines removed", brightflow = { role = "measure", is_kpi = true } },
            changed_files = { datatype = "Integer", brightflow = { role = "measure", is_kpi = true } },
            review_comments = { datatype = "Integer", brightflow = { role = "measure" } },
            comments = { datatype = "Integer", brightflow = { role = "measure" } },
            label_names = { datatype = "String", description = "Comma-joined label names", brightflow = { role = "dimension", label = "Labels" } },
            reviewer_logins = { datatype = "String", description = "Comma-joined requested reviewers", brightflow = { role = "ignored" } },
            milestone_title = { datatype = "String", brightflow = { role = "ignored" } },
            created_at = { datatype = "DateTimeTz", description = "When it was opened", brightflow = { role = "time", label = "Opened" } },
            updated_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            closed_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            author_association = { datatype = "String", brightflow = { role = "dimension" } },
            html_url = { datatype = "String", brightflow = { role = "ignored" } },
        },
        brightflow = {
            display_name = "Pull requests",
            time_granularity = "week",
            comparison_periods = 4,
            doc = { id = "id", number = "number", title = "title", body = "body", timestamp = "created_at", url_template = "{html_url}" },
        },
        map = function(r)
            -- Extract requested reviewer logins
            local reviewer_logins = ""
            if r.requested_reviewers then
                local logins = {}
                for _, rev in ipairs(r.requested_reviewers) do
                    table.insert(logins, rev.login)
                end
                reviewer_logins = table.concat(logins, ",")
            end

            -- Extract label names
            local label_names = ""
            if r.labels then
                local names = {}
                for _, l in ipairs(r.labels) do
                    table.insert(names, l.name)
                end
                label_names = table.concat(names, ",")
            end

            return {
                id = r.id,
                number = r.number,
                title = r.title,
                body = r.body or "",
                state = r.state,
                user_login = r.user and r.user.login or "",
                user_id = r.user and r.user.id or 0,
                draft = r.draft,
                head_ref = r.head and r.head.ref or "",
                head_sha = r.head and r.head.sha or "",
                base_ref = r.base and r.base.ref or "",
                base_sha = r.base and r.base.sha or "",
                merged = r.merged,
                mergeable = r.mergeable,
                merged_at = r.merged_at,
                merged_by_login = r.merged_by and r.merged_by.login or "",
                merge_commit_sha = r.merge_commit_sha or "",
                commits = r.commits or 0,
                additions = r.additions or 0,
                deletions = r.deletions or 0,
                changed_files = r.changed_files or 0,
                review_comments = r.review_comments or 0,
                comments = r.comments or 0,
                label_names = label_names,
                reviewer_logins = reviewer_logins,
                milestone_title = r.milestone and r.milestone.title or "",
                created_at = r.created_at,
                updated_at = r.updated_at,
                closed_at = r.closed_at,
                author_association = r.author_association or "",
                html_url = r.html_url or "",
            }
        end,
    })

    -- Contributors endpoint
    p.endpoint("contributors", {
        path = "/repos/" .. owner .. "/" .. repo .. "/contributors",
        primary_key = {"id"},
        params = {
            per_page = 100,
            anon = "false",
        },
        description = "Contributors to the repository with their commit counts",
        columns = {
            id = { datatype = "Integer", brightflow = { role = "ignored" } },
            login = { datatype = "String", brightflow = { role = "dimension" } },
            type = { datatype = "String", description = "User or Bot", brightflow = { role = "dimension" } },
            contributions = { datatype = "Integer", description = "Commits to the default branch", brightflow = { role = "measure", is_kpi = true } },
            avatar_url = { datatype = "String", brightflow = { role = "ignored" } },
            html_url = { datatype = "String", brightflow = { role = "ignored" } },
            site_admin = { datatype = "Boolean", brightflow = { role = "dimension" } },
        },
        brightflow = {
            display_name = "Contributors",
            doc = { id = "id", title = "login", url_template = "{html_url}" },
        },
        map = function(r)
            return {
                id = r.id,
                login = r.login,
                type = r.type,
                contributions = r.contributions,
                avatar_url = r.avatar_url or "",
                html_url = r.html_url or "",
                site_admin = r.site_admin,
            }
        end,
    })

    -- Issue comments endpoint
    p.endpoint("issue_comments", {
        path = "/repos/" .. owner .. "/" .. repo .. "/issues/comments",
        primary_key = {"id"},
        cursor_field = "updated_at",
        params = {
            per_page = 100,
            sort = "updated",
            direction = "desc",
            since = cursors.issue_comments,  -- nil if no cursor (fetches all)
        },
        description = "Comments on issues and pull requests",
        columns = {
            id = { datatype = "Integer", brightflow = { role = "ignored" } },
            issue_number = { datatype = "Integer", description = "The issue or pull request this comment belongs to", brightflow = { role = "ignored" } },
            user_login = { datatype = "String", brightflow = { role = "dimension", label = "Author" } },
            user_id = { datatype = "Integer", brightflow = { role = "ignored" } },
            body = { datatype = "String", brightflow = { role = "ignored" } },
            created_at = { datatype = "DateTimeTz", description = "When it was posted", brightflow = { role = "time", label = "Posted" } },
            updated_at = { datatype = "DateTimeTz", is_time = false, brightflow = { role = "ignored" } },
            author_association = { datatype = "String", brightflow = { role = "dimension" } },
            reactions_total = { datatype = "Integer", brightflow = { role = "measure", is_kpi = true, label = "Reactions" } },
            html_url = { datatype = "String", brightflow = { role = "ignored" } },
        },
        relationships = {
            issue = { to = "issues", from_columns = { "issue_number" }, to_columns = { "number" } },
        },
        brightflow = {
            display_name = "Issue comments",
            time_granularity = "week",
            comparison_periods = 4,
            doc = { id = "id", body = "body", timestamp = "created_at", url_template = "{html_url}" },
        },
        map = function(r)
            -- Extract issue number from issue_url
            local issue_number = nil
            if r.issue_url then
                issue_number = tonumber(string.match(r.issue_url, "/issues/(%d+)$"))
            end

            return {
                id = r.id,
                issue_number = issue_number,
                user_login = r.user and r.user.login or "",
                user_id = r.user and r.user.id or 0,
                body = r.body or "",
                created_at = r.created_at,
                updated_at = r.updated_at,
                author_association = r.author_association or "",
                reactions_total = r.reactions and r.reactions.total_count or 0,
                html_url = r.html_url or "",
            }
        end,
    })
end
