/**
 * Category icon + tone mapping — one place deciding how each file
 * category looks (tabs, tree rows, chips).
 */
import type { FileCategory } from "../../types";
import {
  ArchiveIcon,
  AudioIcon,
  FileIcon,
  FileImageIcon,
  FileTextIcon,
  MailIcon,
  VideoIcon,
} from "../../shell/icons";

export function CategoryIcon({
  category,
  size = 16,
}: {
  category: FileCategory;
  size?: number;
}) {
  const props = { size, strokeWidth: 1.8, "aria-hidden": true as const };
  switch (category) {
    case "images": return <FileImageIcon {...props} />;
    case "videos": return <VideoIcon {...props} />;
    case "audio": return <AudioIcon {...props} />;
    case "documents": return <FileTextIcon {...props} />;
    case "archives": return <ArchiveIcon {...props} />;
    case "email": return <MailIcon {...props} />;
    default: return <FileIcon {...props} />;
  }
}

/** The CSS tone class per category (tokens: --cat-*). */
export function categoryTone(category: FileCategory): string {
  return `dw-tone-${category}`;
}
