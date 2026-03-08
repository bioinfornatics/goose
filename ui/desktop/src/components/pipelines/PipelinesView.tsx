import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Plus, MoreVertical, Trash2, GitBranch, Workflow } from 'lucide-react';
import { ScrollArea } from '../ui/scroll-area';
import { Button } from '../ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../ui/dialog';
import { listPipelines, createPipeline, deletePipeline } from '../../api';
import type { PipelineManifest } from '../../api/types.gen';

export function PipelinesView() {
  const navigate = useNavigate();
  const [pipelines, setPipelines] = useState<PipelineManifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [deleteTarget, setDeleteTarget] = useState<PipelineManifest | null>(null);

  const fetchPipelines = async () => {
    setLoading(true);
    try {
      const res = await listPipelines();
      if (res.data) {
        setPipelines(res.data.pipelines);
      }
    } catch (err) {
      console.error('Failed to list pipelines', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPipelines();
  }, []);

  const handleCreate = async () => {
    try {
      const res = await createPipeline({
        body: {
          pipeline: {
            apiVersion: 'goose/v1',
            kind: 'Pipeline',
            name: 'Untitled Pipeline',
            description: '',
            nodes: [],
            edges: [],
          },
        },
      });
      if (res.data) {
        navigate(`/pipelines/${res.data.id}`);
      }
    } catch (err) {
      console.error('Failed to create pipeline', err);
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try {
      await deletePipeline({ path: { id: deleteTarget.id } });
      setPipelines((prev) => prev.filter((p) => p.id !== deleteTarget.id));
    } catch (err) {
      console.error('Failed to delete pipeline', err);
    } finally {
      setDeleteTarget(null);
    }
  };

  const formatDate = (dateStr?: string | null) => {
    if (!dateStr) return 'Unknown';
    try {
      return new Date(dateStr).toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
      });
    } catch {
      return 'Unknown';
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-textSubtle">Loading pipelines...</div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-borderSubtle">
        <div className="flex items-center gap-2">
          <Workflow className="size-5 text-textSubtle" />
          <h1 className="text-lg font-semibold">Pipelines</h1>
          <span className="text-sm text-textSubtle">({pipelines.length})</span>
        </div>
        <Button onClick={handleCreate} size="sm">
          <Plus className="size-4" />
          New Pipeline
        </Button>
      </div>

      {/* Content */}
      <ScrollArea className="h-full">
        {pipelines.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-4 py-16 text-center">
            <GitBranch className="size-12 text-textSubtle opacity-40" />
            <div>
              <h2 className="text-lg font-medium mb-1">No pipelines yet</h2>
              <p className="text-sm text-textSubtle max-w-sm">
                Pipelines let you chain agents, tools, and conditions into automated workflows.
                Create your first one to get started.
              </p>
            </div>
            <Button onClick={handleCreate} size="sm">
              <Plus className="size-4" />
              Create Pipeline
            </Button>
          </div>
        ) : (
          <div className="grid gap-3 p-4">
            {pipelines.map((pipeline) => (
              <div
                key={pipeline.id}
                className="flex items-center justify-between p-4 rounded-lg border border-borderSubtle hover:bg-bgSubtle cursor-pointer transition-colors"
                onClick={() => navigate(`/pipelines/${pipeline.id}`)}
              >
                <div className="flex items-center gap-3 min-w-0">
                  <GitBranch className="size-5 text-textSubtle flex-shrink-0" />
                  <div className="min-w-0">
                    <h3 className="font-medium truncate">{pipeline.name}</h3>
                    {pipeline.description && (
                      <p className="text-sm text-textSubtle truncate">{pipeline.description}</p>
                    )}
                    <div className="flex items-center gap-3 mt-1 text-xs text-textSubtle">
                      <span>{pipeline.nodeCount} nodes</span>
                      <span>·</span>
                      <span>Updated {formatDate(pipeline.updatedAt)}</span>
                    </div>
                  </div>
                </div>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="sm"
                      shape="round"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <MoreVertical className="size-4" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem
                      className="text-red-500"
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteTarget(pipeline);
                      }}
                    >
                      <Trash2 className="size-4 mr-2" />
                      Delete
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            ))}
          </div>
        )}
      </ScrollArea>

      {/* Delete confirmation */}
      <Dialog open={!!deleteTarget} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete Pipeline</DialogTitle>
            <DialogDescription>
              Are you sure you want to delete &quot;{deleteTarget?.name}&quot;? This action cannot
              be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDelete}>
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
