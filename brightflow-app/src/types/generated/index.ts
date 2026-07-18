// Auto-generated types from Rust backend via ts-rs
// Regenerate with: TS_RS_EXPORT_DIR=... cargo test --workspace

export type { Aggregation } from './Aggregation';
export type { AggSpec } from './AggSpec';
export type { AvailableConnectorResponse } from './AvailableConnectorResponse';
export type { ColumnInfo } from './ColumnInfo';
export type { ColumnStat } from './ColumnStat';
export type { ConnectorConfig } from './ConnectorConfig';
export type { ConnectorConfigResponse } from './ConnectorConfigResponse';
export type { ConnectorInfo } from './ConnectorInfo';
export type { CreateConnectorConfigRequest } from './CreateConnectorConfigRequest';
export type { CreateJobRequest } from './CreateJobRequest';
export type { DatasetInfo } from './DatasetInfo';
export type { EnrichedSyncRun } from './EnrichedSyncRun';
export type { DatasetMetadataResponse } from './DatasetMetadataResponse';
export type { FilterOp } from './FilterOp';
export type { InsightsResponse } from './InsightsResponse';
export type { AnalysisTree } from './AnalysisTree';
export type { AnalysisNode } from './AnalysisNode';
export type { AnalysisType } from './AnalysisType';
export type { AnalysisCategory } from './AnalysisCategory';
export type { NodeData } from './NodeData';
export type { NodeId } from './NodeId';
export type { NamedSeries } from './NamedSeries';
export type { ScoreBreakdown } from './ScoreBreakdown';
export type { FilterStep } from './FilterStep';
export type { TrendDirection } from './TrendDirection';
export type { EngineConfig } from './EngineConfig';
export type { LoadTableResponse } from './LoadTableResponse';
export type { Operation } from './Operation';
export type { PresetInfo } from './PresetInfo';
export type { PresetScheduleRequest } from './PresetScheduleRequest';
export type { Query } from './Query';
export type { QueryResponse } from './QueryResponse';
export type { ReviewRequest } from './ReviewRequest';
export type { RunRequest } from './RunRequest';
export type { RunTriggerResponse } from './RunTriggerResponse';
export type { ScheduleRequest } from './ScheduleRequest';
export type { ScheduleResponse } from './ScheduleResponse';
export type { SchedulerJob } from './SchedulerJob';
export type { SyncRun } from './SyncRun';
export type { SyncState } from './SyncState';
export type { TableInfo } from './TableInfo';
export type { TrendsRequest } from './TrendsRequest';
export type { TriggerRunResponse } from './TriggerRunResponse';
export type { UnifiedConnector } from './UnifiedConnector';
export type { UnifiedJob } from './UnifiedJob';
export type { UnifiedSyncRun } from './UnifiedSyncRun';
export type { UpdateConnectorConfigRequest } from './UpdateConnectorConfigRequest';
export type { UpdateJobRequest } from './UpdateJobRequest';
export type { UpdateTokenRequest } from './UpdateTokenRequest';
export type { UploadResponse } from './UploadResponse';
export type { User } from './User';
export type { WsClientMessage } from './WsClientMessage';
export type { WsServerMessage } from './WsServerMessage';

// Event ingestion types
export type { BreakdownRow } from './BreakdownRow';
export type { CreateSourceRequest } from './CreateSourceRequest';
export type { DashboardStats } from './DashboardStats';
export type { Source } from './Source';
export type { TimeseriesPoint } from './TimeseriesPoint';
export type { UpdateSourceRequest } from './UpdateSourceRequest';

// Product analytics types
export type { EventListRow } from './EventListRow';
export type { FunnelRequest } from './FunnelRequest';
export type { FunnelResult } from './FunnelResult';
export type { FunnelStep } from './FunnelStep';
export type { FunnelStepResult } from './FunnelStepResult';
export type { RetentionRequest } from './RetentionRequest';
export type { RetentionResult } from './RetentionResult';
export type { RetentionRow } from './RetentionRow';
export type { UserProfile } from './UserProfile';
export type { UserTimelineEvent } from './UserTimelineEvent';

// Topics types
export type { ClusterDetail } from './ClusterDetail';
export type { ClusterSummary } from './ClusterSummary';
export type { DocRef } from './DocRef';
export type { LabelBucket } from './LabelBucket';
export type { ReclusterRequest } from './ReclusterRequest';
export type { TopicsOverview } from './TopicsOverview';

// Text Explorer types
export type { SearchTerm } from './SearchTerm';
export type { TextExploreRequest } from './TextExploreRequest';
export type { TextExploreResponse } from './TextExploreResponse';
export type { TextExploreRow } from './TextExploreRow';
export type { TextRun } from './TextRun';
export type { WordScore } from './WordScore';

// Intent taxonomy types
export type { CurationDoc } from './CurationDoc';
export type { CurationQueue } from './CurationQueue';
export type { CurationQueueQuery } from './CurationQueueQuery';
export type { TaxonomyCategory } from './TaxonomyCategory';
export type { TaxonomyOverview } from './TaxonomyOverview';
export type { Action } from './Action';
export type { ActionLogEntry } from './ActionLogEntry';
export type { ActionManifestEntry } from './ActionManifestEntry';
export type { ActionRequest } from './ActionRequest';
export type { ActionResponse } from './ActionResponse';
export type { ActionStatus } from './ActionStatus';
export type { BulkApproveFailure } from './BulkApproveFailure';
export type { BulkApproveResponse } from './BulkApproveResponse';
export type { PendingCount } from './PendingCount';
export type { DismissReason } from './DismissReason';
export type { SuppressKind } from './SuppressKind';
export type { EnrichmentSettingsResponse } from './EnrichmentSettingsResponse';
export type { UpdateEnrichmentSettingsRequest } from './UpdateEnrichmentSettingsRequest';
export type { LanguageBucket } from './LanguageBucket';
export type { ProvenanceStep } from './ProvenanceStep';
export type { LlmProviderResponse } from './LlmProviderResponse';
export type { LlmTestResponse } from './LlmTestResponse';
export type { UpsertLlmProviderRequest } from './UpsertLlmProviderRequest';
export type { AgentRunResponse } from './AgentRunResponse';
export type { StartAgentRunRequest } from './StartAgentRunRequest';
