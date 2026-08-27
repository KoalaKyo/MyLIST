/**
 * Type-safe mirror of the Rust MCP contract.
 *
 * This file intentionally contains types and constants only. It must not open
 * a transport, call IPC, read files, access SQLite, or mutate application
 * state. The bridge implementation will be added in a later stage.
 */

export const MCP_PROTOCOL_VERSION = "mylist.mcp.v1" as const;

export type McpRisk = "read" | "write" | "high_risk";

export type McpErrorCode =
  | "MCP_SERVICE_DISABLED"
  | "MCP_SERVICE_UNAVAILABLE"
  | "MCP_PROTOCOL_MISMATCH"
  | "MCP_INVALID_REQUEST"
  | "MCP_INVALID_ARGUMENT"
  | "MCP_NOT_FOUND"
  | "MCP_CONFLICT"
  | "MCP_CONFIRMATION_REQUIRED"
  | "MCP_CONFIRMATION_EXPIRED"
  | "MCP_OPERATION_REJECTED"
  | "MCP_RATE_LIMITED"
  | "MCP_INTERNAL_ERROR";

export type McpToolDescriptor = {
  name: string;
  risk: McpRisk;
  requiresConfirmation: boolean;
  descriptionKey: string;
};

export type McpContract = {
  protocolVersion: typeof MCP_PROTOCOL_VERSION;
  tools: readonly McpToolDescriptor[];
};

export type McpRequestMeta = {
  requestId: string;
  protocolVersion: typeof MCP_PROTOCOL_VERSION;
};

export type McpError = {
  code: McpErrorCode;
  messageKey: `mcp.error.${McpErrorCode}`;
  retryable: boolean;
};

export const MCP_ERROR_KEYS: Readonly<Record<McpErrorCode, McpError["messageKey"]>> = {
  MCP_SERVICE_DISABLED: "mcp.error.MCP_SERVICE_DISABLED",
  MCP_SERVICE_UNAVAILABLE: "mcp.error.MCP_SERVICE_UNAVAILABLE",
  MCP_PROTOCOL_MISMATCH: "mcp.error.MCP_PROTOCOL_MISMATCH",
  MCP_INVALID_REQUEST: "mcp.error.MCP_INVALID_REQUEST",
  MCP_INVALID_ARGUMENT: "mcp.error.MCP_INVALID_ARGUMENT",
  MCP_NOT_FOUND: "mcp.error.MCP_NOT_FOUND",
  MCP_CONFLICT: "mcp.error.MCP_CONFLICT",
  MCP_CONFIRMATION_REQUIRED: "mcp.error.MCP_CONFIRMATION_REQUIRED",
  MCP_CONFIRMATION_EXPIRED: "mcp.error.MCP_CONFIRMATION_EXPIRED",
  MCP_OPERATION_REJECTED: "mcp.error.MCP_OPERATION_REJECTED",
  MCP_RATE_LIMITED: "mcp.error.MCP_RATE_LIMITED",
  MCP_INTERNAL_ERROR: "mcp.error.MCP_INTERNAL_ERROR",
};

export const isHighRisk = (risk: McpRisk): boolean => risk === "high_risk";
