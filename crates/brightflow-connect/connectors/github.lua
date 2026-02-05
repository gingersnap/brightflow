-- GitHub Connector for Avon
-- Extracts repository, issues, pull requests, and contributors data
-- Config: token, owner, repo, output_path

return function(p)
    local owner = p.config.owner
    local repo = p.config.repo

    p.base_url("https://api.github.com")

    -- Auth: GitHub expects "token xxx" not "Bearer xxx"
    p.auth(function(ctx)
        ctx.headers["Authorization"] = "token " .. p.config.token
        ctx.headers["Accept"] = "application/vnd.github+json"
        ctx.headers["X-GitHub-Api-Version"] = "2022-11-28"
        ctx.headers["User-Agent"] = "Avon/0.1.0"
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
        params = {
            state = "all",
            per_page = 100,
            sort = "updated",
            direction = "desc",
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
        params = {
            state = "all",
            per_page = 100,
            sort = "updated",
            direction = "desc",
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
        params = {
            per_page = 100,
            anon = "false",
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
        params = {
            per_page = 100,
            sort = "updated",
            direction = "desc",
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
