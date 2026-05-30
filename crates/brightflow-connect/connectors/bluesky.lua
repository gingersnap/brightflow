--[[ @longbow
name = "bluesky"
version = "0.1.0"
description = "Bluesky posts and profile snapshots (authenticated via App Password)"
]]

-- Flatten a Bluesky `postView` into a Parquet row. `is_repost` is supplied by
-- the caller (true for feedViewPost items whose reason is reasonRepost).
local function flatten_post(pv, is_repost)
    local rec = pv.record or {}
    local author = pv.author or {}

    -- First language code from record.langs[]
    local lang = ""
    if rec.langs and rec.langs[1] then
        lang = rec.langs[1]
    end

    -- Walk facets[].features[] to collect hashtags / mentions / link URIs.
    -- Lexicon $type values:
    --   app.bsky.richtext.facet#tag       -> feat.tag
    --   app.bsky.richtext.facet#mention   -> feat.did
    --   app.bsky.richtext.facet#link      -> feat.uri
    local hashtags = {}
    local mentions = {}
    if rec.facets then
        for _, facet in ipairs(rec.facets) do
            if facet.features then
                for _, feat in ipairs(facet.features) do
                    local ft = feat["$type"]
                    if ft == "app.bsky.richtext.facet#tag" then
                        if feat.tag then table.insert(hashtags, feat.tag) end
                    elseif ft == "app.bsky.richtext.facet#mention" then
                        if feat.did then table.insert(mentions, feat.did) end
                    end
                end
            end
        end
    end

    -- Embed detection. Hydrated views in pv.embed use `*#view` suffix; raw
    -- record embeds in rec.embed don't. Check both, prefer hydrated.
    local embed_type = ""
    if pv.embed and pv.embed["$type"] then
        embed_type = pv.embed["$type"]
    elseif rec.embed and rec.embed["$type"] then
        embed_type = rec.embed["$type"]
    end
    local has_image = embed_type:find("app%.bsky%.embed%.images") ~= nil
        or embed_type:find("app%.bsky%.embed%.recordWithMedia") ~= nil
    local has_video = embed_type:find("app%.bsky%.embed%.video") ~= nil

    -- External link URI from embed (hydrated or raw record).
    local external_uri = ""
    if pv.embed and pv.embed.external and pv.embed.external.uri then
        external_uri = pv.embed.external.uri
    elseif rec.embed and rec.embed.external and rec.embed.external.uri then
        external_uri = rec.embed.external.uri
    end

    return {
        uri = pv.uri,
        cid = pv.cid,
        author_did = author.did or "",
        author_handle = author.handle or "",
        text = rec.text or "",
        created_at = rec.createdAt or "",
        indexed_at = pv.indexedAt or "",
        lang = lang,
        reply_count = pv.replyCount or 0,
        repost_count = pv.repostCount or 0,
        like_count = pv.likeCount or 0,
        quote_count = pv.quoteCount or 0,
        is_reply = rec.reply ~= nil,
        is_repost = is_repost and true or false,
        hashtags = table.concat(hashtags, ","),
        mentions = table.concat(mentions, ","),
        external_uri = external_uri,
        has_image = has_image,
        has_video = has_video,
    }
end

return function(p)
    local cursors = p.config._cursors or {}
    local mode = p.config.mode
    assert(
        mode == "keyword" or mode == "actor",
        "bluesky connector: 'mode' must be 'keyword' or 'actor' (got " .. tostring(mode) .. ")"
    )

    -- Authentication. The public AppView serves the first page of `searchPosts`
    -- unauthenticated but returns 403 on any request carrying a `cursor`, so
    -- pagination (and reliable access generally) requires a logged-in session.
    -- We log in with an App Password via com.atproto.server.createSession and let
    -- the user's PDS proxy `app.bsky.*` reads to the AppView.
    -- Trim surrounding whitespace (pasted values often carry a trailing newline,
    -- which Bluesky rejects with a 401) and strip a leading "@" from handles.
    local function trim(s)
        if s == nil then return nil end
        return (s:gsub("^%s+", ""):gsub("%s+$", ""))
    end

    local identifier = trim(p.config.identifier)
    if identifier ~= nil then
        identifier = (identifier:gsub("^@", ""))   -- "@alice.bsky.social" -> "alice.bsky.social"
    end
    local password = trim(p.config.token)          -- Bluesky App Password (xxxx-xxxx-xxxx-xxxx)
    local service = trim(p.config.service)
    if service == nil or service == "" then
        service = "https://bsky.social"
    end
    assert(
        identifier and identifier ~= "",
        "bluesky connector: 'identifier' (login handle/DID/email) is required"
    )
    assert(
        password and password ~= "",
        "bluesky connector: an App Password (stored as the source token) is required"
    )

    p.base_url(service)
    p.auth(auth.atproto_session({
        service = service,
        identifier = identifier,
        password = password,
    }))

    -- Tell the PDS to proxy these app.bsky.* read requests to the AppView.
    local appview_headers = { ["atproto-proxy"] = "did:web:api.bsky.app#bsky_appview" }

    -- Bluesky returns the next cursor as a top-level `cursor` string in the
    -- response body, and accepts it back as a `?cursor=...` query parameter.
    -- The cursor is omitted when no more results exist; paginate.cursor stops.
    p.paginate(paginate.cursor({ path = "cursor", param = "cursor" }))

    p.retry(retry.exponential({
        max_attempts = 3,
        base_delay = 1000,
    }))

    p.output(output.parquet({
        path = p.config.output_path,
    }))

    if mode == "keyword" then
        local query = p.config.query
        assert(
            query and query ~= "",
            "bluesky connector: 'query' is required in keyword mode"
        )
        local lang = p.config.lang
        if lang == "" then lang = nil end

        p.endpoint("posts", {
            path = "/xrpc/app.bsky.feed.searchPosts",
            headers = appview_headers,
            records_path = "posts",
            primary_key = {"uri"},
            cursor_field = "indexed_at",
            params = {
                q = query,
                sort = "latest",
                lang = lang,                    -- nil keys are omitted from query
                since = cursors.posts,          -- server-side filter on sortAt (≈ indexedAt)
                limit = 100,
            },
            map = function(post)
                return flatten_post(post, false)
            end,
        })
    else
        local actor = p.config.actor
        assert(
            actor and actor ~= "",
            "bluesky connector: 'actor' is required in actor mode"
        )

        p.endpoint("posts", {
            path = "/xrpc/app.bsky.feed.getAuthorFeed",
            headers = appview_headers,
            records_path = "feed",
            primary_key = {"uri"},
            cursor_field = "indexed_at",
            params = {
                actor = actor,                          -- handle or DID
                filter = "posts_with_replies",          -- default; includes reposts
                limit = 100,
            },
            map = function(item)
                local post = item.post
                local is_repost = item.reason
                    and item.reason["$type"] == "app.bsky.feed.defs#reasonRepost"
                return flatten_post(post, is_repost)
            end,
        })

        -- Daily profile snapshot. PK includes snapshot_date so merge_parquet
        -- upserts idempotently per UTC day regardless of sync interval.
        p.endpoint("profile_snapshot", {
            path = "/xrpc/app.bsky.actor.getProfile",
            headers = appview_headers,
            primary_key = {"did", "snapshot_date"},
            params = { actor = actor },
            map = function(r)
                return {
                    did = r.did,
                    handle = r.handle or "",
                    display_name = r.displayName or "",
                    description = r.description or "",
                    followers_count = r.followersCount or 0,
                    follows_count = r.followsCount or 0,
                    posts_count = r.postsCount or 0,
                    indexed_at = r.indexedAt or "",
                    created_at = r.createdAt or "",
                    snapshot_date = os.date("!%Y-%m-%d"),
                }
            end,
        })
    end
end
