import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ReactFlowProvider } from "@xyflow/react";
import { ArrowLeft, Play, Square, Loader2, CheckCircle2, XCircle, Clock } from "lucide-react";
import { Button } from "../ui/button";
import { ScrollArea } from "../ui/scroll-area";
import { getPipeline, updatePipeline } from "../../api";
import type { Pipeline } from "../../api";
import { NodePalette } from "./NodePalette";
import { PipelineEditorCanvas } from "./PipelineEditorCanvas";

interface NodeStatus {
  status: "pending" | "running" | "completed" | "failed";
  output?: string;
  error?: string;
  durationMs?: number;
}

export function PipelineEditorPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [pipeline, setPipeline] = useState<Pipeline | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Execution state
  const [running, setRunning] = useState(false);
  const [nodeStatuses, setNodeStatuses] = useState<Record<string, NodeStatus>>({});
  const [runStatus, setRunStatus] = useState<string | null>(null);
  const [totalDuration, setTotalDuration] = useState<number | null>(null);
  const [showResults, setShowResults] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!id) return;
    setLoading(true);
    getPipeline({ path: { id } })
      .then((res) => {
        if (res.data) {
          setPipeline(res.data.pipeline);
        } else {
          setError("Pipeline not found");
        }
      })
      .catch(() => setError("Failed to load pipeline"))
      .finally(() => setLoading(false));
  }, [id]);

  const handleSave = useCallback(
    async (updated: Pipeline) => {
      if (!id) return;
      setSaving(true);
      try {
        await updatePipeline({
          path: { id },
          body: { pipeline: updated },
        });
        setPipeline(updated);
      } finally {
        setSaving(false);
      }
    },
    [id]
  );

  const handleRun = useCallback(async () => {
    if (!id || !pipeline) return;

    setRunning(true);
    setShowResults(true);
    setRunStatus(null);
    setTotalDuration(null);

    // Initialize all nodes as pending
    const initial: Record<string, NodeStatus> = {};
    for (const node of pipeline.nodes) {
      initial[node.id] = { status: "pending" };
    }
    setNodeStatuses(initial);

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      const apiHost = await window.electron.getGoosedHostPort();
      const secretKey = await window.electron.getSecretKey();

      const response = await fetch(`${apiHost}/pipelines/${id}/run`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-Secret-Key": secretKey,
        },
        body: JSON.stringify({ max_concurrency: 4 }),
        signal: controller.signal,
      });

      if (!response.ok) {
        setRunStatus("failed");
        setRunning(false);
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) {
        setRunStatus("failed");
        setRunning(false);
        return;
      }

      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          if (!line.startsWith("data:")) continue;
          const jsonStr = line.slice(5).trim();
          if (!jsonStr) continue;

          try {
            const event = JSON.parse(jsonStr);
            switch (event.type) {
              case "node_started":
                setNodeStatuses((prev) => ({
                  ...prev,
                  [event.node_id]: { status: "running" },
                }));
                break;
              case "node_completed":
                setNodeStatuses((prev) => ({
                  ...prev,
                  [event.node_id]: {
                    status: "completed",
                    output: event.output,
                    durationMs: event.duration_ms,
                  },
                }));
                break;
              case "node_failed":
                setNodeStatuses((prev) => ({
                  ...prev,
                  [event.node_id]: {
                    status: "failed",
                    error: event.error,
                    durationMs: event.duration_ms,
                  },
                }));
                break;
              case "run_completed":
                setRunStatus(event.status);
                setTotalDuration(event.total_duration_ms);
                break;
            }
          } catch {
            // Skip malformed events
          }
        }
      }
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") {
        setRunStatus("cancelled");
      } else {
        setRunStatus("failed");
      }
    } finally {
      setRunning(false);
      abortRef.current = null;
    }
  }, [id, pipeline]);

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error || !pipeline) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4">
        <p className="text-destructive">{error || "Pipeline not found"}</p>
        <Button variant="outline" onClick={() => navigate("/pipelines")}>
          Back to Pipelines
        </Button>
      </div>
    );
  }

  const statusIcon = (s: NodeStatus["status"]) => {
    switch (s) {
      case "pending":
        return <Clock className="w-4 h-4 text-muted-foreground" />;
      case "running":
        return <Loader2 className="w-4 h-4 animate-spin text-blue-500" />;
      case "completed":
        return <CheckCircle2 className="w-4 h-4 text-green-500" />;
      case "failed":
        return <XCircle className="w-4 h-4 text-red-500" />;
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-3 px-4 py-3 border-b">
        <Button variant="ghost" size="sm" onClick={() => navigate("/pipelines")}>
          <ArrowLeft className="w-4 h-4" />
        </Button>
        <h1 className="text-lg font-semibold flex-1 truncate">{pipeline.name}</h1>
        <div className="flex items-center gap-2">
          {running ? (
            <Button variant="destructive" size="sm" onClick={handleStop}>
              <Square className="w-4 h-4 mr-1" />
              Stop
            </Button>
          ) : (
            <Button variant="default" size="sm" onClick={handleRun}>
              <Play className="w-4 h-4 mr-1" />
              Run
            </Button>
          )}
          <Button variant="outline" size="sm" disabled={saving}>
            {saving ? "Saving..." : "Save"}
          </Button>
        </div>
      </div>

      {/* Main content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Palette */}
        <div className="w-56 border-r overflow-y-auto">
          <NodePalette />
        </div>

        {/* Canvas */}
        <div className="flex-1 relative">
          <ReactFlowProvider>
            <PipelineEditorCanvas
              pipeline={pipeline}
              onSave={handleSave}
            />
          </ReactFlowProvider>
        </div>

        {/* Run Results Panel */}
        {showResults && (
          <div className="w-80 border-l flex flex-col">
            <div className="flex items-center justify-between px-3 py-2 border-b">
              <span className="text-sm font-medium">Run Results</span>
              <Button
                variant="ghost"
                size="xs"
                onClick={() => setShowResults(false)}
              >
                ✕
              </Button>
            </div>

            {/* Overall status */}
            {runStatus && (
              <div
                className={`px-3 py-2 text-sm border-b ${
                  runStatus === "completed"
                    ? "bg-green-50 text-green-700 dark:bg-green-950 dark:text-green-300"
                    : runStatus === "failed"
                      ? "bg-red-50 text-red-700 dark:bg-red-950 dark:text-red-300"
                      : "bg-yellow-50 text-yellow-700 dark:bg-yellow-950 dark:text-yellow-300"
                }`}
              >
                {runStatus === "completed" ? "✓ Pipeline completed" : `Pipeline ${runStatus}`}
                {totalDuration != null && ` (${(totalDuration / 1000).toFixed(1)}s)`}
              </div>
            )}

            {/* Node statuses */}
            <ScrollArea className="flex-1">
              <div className="p-2 space-y-1">
                {pipeline.nodes.map((node) => {
                  const ns = nodeStatuses[node.id];
                  if (!ns) return null;
                  return (
                    <div
                      key={node.id}
                      className="flex items-start gap-2 p-2 rounded text-sm border"
                    >
                      <div className="mt-0.5">{statusIcon(ns.status)}</div>
                      <div className="flex-1 min-w-0">
                        <div className="font-medium truncate">{node.label}</div>
                        {ns.durationMs != null && (
                          <div className="text-xs text-muted-foreground">
                            {(ns.durationMs / 1000).toFixed(1)}s
                          </div>
                        )}
                        {ns.output && (
                          <div className="text-xs text-muted-foreground mt-1 line-clamp-3">
                            {ns.output}
                          </div>
                        )}
                        {ns.error && (
                          <div className="text-xs text-red-500 mt-1 line-clamp-3">{ns.error}</div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </ScrollArea>
          </div>
        )}
      </div>
    </div>
  );
}
