import { Component, ErrorInfo, ReactNode } from "react";
import { AlertOctagon } from "lucide-react";
import { Button } from "@/components/ui/button";

interface Props {
  children?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("ErrorBoundary caught an error:", error, errorInfo);
  }

  public render() {
    if (this.state.hasError) {
      return (
        <div className="flex flex-col items-center justify-center min-h-[400px] p-6 text-center bg-background text-foreground">
          <div className="p-4 bg-destructive/10 text-destructive rounded-full mb-4">
            <AlertOctagon className="w-12 h-12" />
          </div>
          <h2 className="text-xl font-semibold mb-2">Đã xảy ra lỗi hiển thị</h2>
          <p className="text-sm text-muted-foreground max-w-md mb-4 leading-relaxed">
            {this.state.error?.toString() || "Lỗi không xác định"}
          </p>
          {this.state.error?.stack && (
            <pre className="text-[10px] text-left bg-muted p-4 rounded-lg max-w-2xl overflow-auto max-h-[150px] font-mono text-muted-foreground border border-border/20 mb-6">
              {this.state.error.stack}
            </pre>
          )}
          <Button
            onClick={() => this.setState({ hasError: false, error: null })}
            variant="outline"
          >
            Thử lại
          </Button>
        </div>
      );
    }

    return this.props.children;
  }
}
