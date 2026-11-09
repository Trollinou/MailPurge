export interface AccountRecord {
  id: number;
  email: string;
  provider_type: string;
  display_name: string;
}

export interface DomainStats {
  root_domain: string;
  total_emails: number;
  total_size: number;
  purgeable_count: number;
  purgeable_size: number;
  protected_count: number;
}

export type EmailClassification =
  | "PURGEABLE"
  | "PROTECTED_INVOICE"
  | "PROTECTED_FLAGGED"
  | "PROTECTED_USER"
  | "RECENT";

export type DomainSortOption = "size" | "count" | "alpha";

export interface EmailRecord {
  account_id: number;
  folder_path: string;
  uid: number;
  message_id?: string;
  root_domain: string;
  full_domain: string;
  sender_address: string;
  subject: string;
  received_at: number;
  size_bytes: number;
  has_unsubscribe: boolean;
  has_attachment: boolean;
  attachment_names: string[];
  is_flagged: boolean;
  classification: EmailClassification;
  body_snippet?: string;
}

export interface PurgeReport {
  total_candidates: number;
  purged_count: number;
  purged_bytes: number;
  skipped_protected_count: number;
  batches_executed: number;
  is_dry_run: boolean;
  details_by_folder: Record<string, number[]>;
}
