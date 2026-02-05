-- GitHub Connector for Avon
-- Extracts repository, issues, pull requests, and contributors data

local owner = ctx:get_config("owner")
local repo = ctx:get_config("repo")
local token = ctx:get_config("token")
local output_path = ctx:get_config("output_path") or "./data/github"

-- Pipeline configuration
p:base_url("https://api.github.com")
p:auth(auth.bearer(token))
p:paginate(paginate.link_header())
p:rate_limit(rate_limit.header({
    remaining = "X-RateLimit-Remaining",
    reset = "X-RateLimit-Reset",
    threshold = 100
}))
p:retry(retry.exponential({
    max_attempts = 3,
    base_delay = 1000,
    max_delay = 30000
}))
p:output(output.parquet({ path = output_path }))

-- Repository endpoint (single object, wrapped in array for consistency)
p:endpoint("repository", {
    path = "/repos/" .. owner .. "/" .. repo,
    headers = {
        ["Accept"] = "application/vnd.github+json",
        ["X-GitHub-Api-Version"] = "2022-11-28"
    },
    map = function(record)
        return {
            id = record.id,
            name = record.name,
            full_name = record.full_name,
            description = record.description or "",
            owner_login = record.owner and record.owner.login or "",
            owner_id = record.owner and record.owner.id or 0,
            private = record.private,
            fork = record.fork,
            created_at = record.created_at,
            updated_at = record.updated_at,
            pushed_at = record.pushed_at,
            stargazers_count = record.stargazers_count or 0,
            watchers_count = record.watchers_count or 0,
            forks_count = record.forks_count or 0,
            open_issues_count = record.open_issues_count or 0,
            language = record.language or "",
            topics = record.topics and table.concat(record.topics, ",") or "",
            default_branch = record.default_branch or "main",
            has_issues = record.has_issues,
            has_projects = record.has_projects,
            has_wiki = record.has_wiki,
            archived = record.archived,
            disabled = record.disabled,
            visibility = record.visibility or "public",
            size = record.size or 0,
            subscribers_count = record.subscribers_count or 0,
            network_count = record.network_count or 0
        }
    end,
    -- Wrap single object in array
    records_path = nil  -- Single object, not array
})

-- Issues endpoint (includes both issues and PRs)
p:endpoint("issues", {
    path = "/repos/" .. owner .. "/" .. repo .. "/issues",
    params = {
        state = "all",
        per_page = 100,
        sort = "updated",
        direction = "desc"
    },
    headers = {
        ["Accept"] = "application/vnd.github+json",
        ["X-GitHub-Api-Version"] = "2022-11-28"
    },
    map = function(record)
        -- Extract assignee logins
        local assignee_logins = ""
        if record.assignees then
            local logins = {}
            for _, a in ipairs(record.assignees) do
                table.insert(logins, a.login)
            end
            assignee_logins = table.concat(logins, ",")
        end

        -- Extract label names
        local label_names = ""
        if record.labels then
            local names = {}
            for _, l in ipairs(record.labels) do
                table.insert(names, l.name)
            end
            label_names = table.concat(names, ",")
        end

        return {
            id = record.id,
            number = record.number,
            title = record.title,
            body = record.body or "",
            state = record.state,
            state_reason = record.state_reason or "",
            user_login = record.user and record.user.login or "",
            user_id = record.user and record.user.id or 0,
            assignee_logins = assignee_logins,
            label_names = label_names,
            milestone_title = record.milestone and record.milestone.title or "",
            milestone_number = record.milestone and record.milestone.number or nil,
            comments = record.comments or 0,
            is_pull_request = record.pull_request ~= nil,
            created_at = record.created_at,
            updated_at = record.updated_at,
            closed_at = record.closed_at,
            author_association = record.author_association or "",
            locked = record.locked,
            active_lock_reason = record.active_lock_reason or "",
            reactions_total = record.reactions and record.reactions.total_count or 0,
            reactions_plus_one = record.reactions and record.reactions["+1"] or 0,
            reactions_minus_one = record.reactions and record.reactions["-1"] or 0,
            reactions_heart = record.reactions and record.reactions.heart or 0,
            html_url = record.html_url or ""
        }
    end
})

-- Pull Requests endpoint (PR-specific data)
p:endpoint("pull_requests", {
    path = "/repos/" .. owner .. "/" .. repo .. "/pulls",
    params = {
        state = "all",
        per_page = 100,
        sort = "updated",
        direction = "desc"
    },
    headers = {
        ["Accept"] = "application/vnd.github+json",
        ["X-GitHub-Api-Version"] = "2022-11-28"
    },
    map = function(record)
        -- Extract requested reviewer logins
        local reviewer_logins = ""
        if record.requested_reviewers then
            local logins = {}
            for _, r in ipairs(record.requested_reviewers) do
                table.insert(logins, r.login)
            end
            reviewer_logins = table.concat(logins, ",")
        end

        -- Extract label names
        local label_names = ""
        if record.labels then
            local names = {}
            for _, l in ipairs(record.labels) do
                table.insert(names, l.name)
            end
            label_names = table.concat(names, ",")
        end

        return {
            id = record.id,
            number = record.number,
            title = record.title,
            body = record.body or "",
            state = record.state,
            user_login = record.user and record.user.login or "",
            user_id = record.user and record.user.id or 0,
            draft = record.draft,
            head_ref = record.head and record.head.ref or "",
            head_sha = record.head and record.head.sha or "",
            base_ref = record.base and record.base.ref or "",
            base_sha = record.base and record.base.sha or "",
            merged = record.merged,
            mergeable = record.mergeable,
            rebaseable = record.rebaseable,
            merged_at = record.merged_at,
            merged_by_login = record.merged_by and record.merged_by.login or "",
            merge_commit_sha = record.merge_commit_sha or "",
            commits = record.commits or 0,
            additions = record.additions or 0,
            deletions = record.deletions or 0,
            changed_files = record.changed_files or 0,
            review_comments = record.review_comments or 0,
            comments = record.comments or 0,
            maintainer_can_modify = record.maintainer_can_modify,
            label_names = label_names,
            reviewer_logins = reviewer_logins,
            milestone_title = record.milestone and record.milestone.title or "",
            created_at = record.created_at,
            updated_at = record.updated_at,
            closed_at = record.closed_at,
            author_association = record.author_association or "",
            html_url = record.html_url or ""
        }
    end
})

-- Contributors endpoint
p:endpoint("contributors", {
    path = "/repos/" .. owner .. "/" .. repo .. "/contributors",
    params = {
        per_page = 100,
        anon = "false"
    },
    headers = {
        ["Accept"] = "application/vnd.github+json",
        ["X-GitHub-Api-Version"] = "2022-11-28"
    },
    map = function(record)
        return {
            id = record.id,
            login = record.login,
            type = record.type,
            contributions = record.contributions,
            avatar_url = record.avatar_url or "",
            html_url = record.html_url or "",
            site_admin = record.site_admin
        }
    end
})

-- Issue comments endpoint
p:endpoint("issue_comments", {
    path = "/repos/" .. owner .. "/" .. repo .. "/issues/comments",
    params = {
        per_page = 100,
        sort = "updated",
        direction = "desc"
    },
    headers = {
        ["Accept"] = "application/vnd.github+json",
        ["X-GitHub-Api-Version"] = "2022-11-28"
    },
    map = function(record)
        -- Extract issue number from issue_url
        local issue_number = nil
        if record.issue_url then
            issue_number = tonumber(string.match(record.issue_url, "/issues/(%d+)$"))
        end

        return {
            id = record.id,
            issue_number = issue_number,
            user_login = record.user and record.user.login or "",
            user_id = record.user and record.user.id or 0,
            body = record.body or "",
            created_at = record.created_at,
            updated_at = record.updated_at,
            author_association = record.author_association or "",
            reactions_total = record.reactions and record.reactions.total_count or 0,
            reactions_plus_one = record.reactions and record.reactions["+1"] or 0,
            reactions_minus_one = record.reactions and record.reactions["-1"] or 0,
            reactions_heart = record.reactions and record.reactions.heart or 0,
            html_url = record.html_url or ""
        }
    end
})
