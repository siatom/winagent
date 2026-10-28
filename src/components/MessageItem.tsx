import { useState } from "react";
import { BrainCircuit, ChevronRight } from "lucide-react";
import type { Message } from "../lib/types";
import { cn, formatTime } from "../lib/utils";
import { Logo } from "./Logo";
import { Markdown } from "./Markdown";
import { ToolCard } from "./ToolCard";

export function ReasoningBlock({ text, live }: { text: string; live?: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="mb-2 overflow-hidden rounded-[10px] border border-line bg-surface-2/40">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-1.5 px-2.5 py-1.5 text-[11.5px] text-dim transition-colors hover:text-muted"
      >
        <BrainCircuit size={12} className={cn(live && "text-accent")} />
        思考过程
        {live && <span className="pulse-dot h-1 w-1 rounded-full bg-accent" />}
        <ChevronRight size={12} className={cn("ml-auto transition-transform", open && "rotate-90")} />
      </button>
      {open && (
        <div className="max-h-[320px] overflow-auto whitespace-pre-wrap border-t border-line/60 px-3 py-2 text-[12px] leading-relaxed text-muted">
          {text}
        </div>
      )}
    </div>
  );
}

function TypingDots() {
  return (
    <div className="flex items-center gap-1 py-1.5">
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="pulse-dot h-1.5 w-1.5 rounded-full bg-accent"
          style={{ animationDelay: `${i * 0.18}s` }}
        />
      ))}
    </div>
  );
}

export function StreamingMessage({ text, reasoning }: { text: string; reasoning: string }) {
  return (
    <div className="animate-fade-up flex gap-3">
      <Logo size={22} className="mt-0.5" />
      <div className="min-w-0 flex-1">
        <div className="mb-1 flex items-center gap-2 text-[11.5px] text-dim">
          <span className="font-medium text-fg">Windows God</span>
          <span>正在处理</span>
        </div>
        {reasoning && <ReasoningBlock text={reasoning} live />}
        {text ? <Markdown>{text}</Markdown> : <TypingDots />}
      </div>
    </div>
  );
}

export function MessageItem({ message }: { message: Message }) {
  if (message.role === "user") {
    return (
      <div className="animate-fade-up flex justify-end">
        <div className="max-w-[82%] rounded-[14px] rounded-tr-[4px] border border-line-strong bg-surface-3 px-3.5 py-2 text-[14px] leading-relaxed text-fg whitespace-pre-wrap">
          {message.content}
        </div>
      </div>
    );
  }

  if (message.role === "tool") {
    return (
      <div className="pl-[34px]">
        <ToolCard name={message.name ?? "tool"} meta={message.meta ?? undefined} />
      </div>
    );
  }

  const hasContent = message.content.trim().length > 0;

  return (
    <div className="animate-fade-up flex gap-3">
      <Logo size={22} className="mt-0.5" />
      <div className="min-w-0 flex-1">
        <div className="mb-1 flex items-center gap-2 text-[11.5px] text-dim">
          <span className="font-medium text-fg">Windows God</span>
          <span>{formatTime(message.createdAt)}</span>
        </div>
        {message.reasoning && <ReasoningBlock text={message.reasoning} />}
        {hasContent ? (
          <Markdown>{message.content}</Markdown>
        ) : (
          <div className="text-[12.5px] text-dim">（正在调用工具…）</div>
        )}
      </div>
    </div>
  );
}
