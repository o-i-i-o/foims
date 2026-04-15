export interface User {
  id: number;
  username: string;
  email: string;
  role: "admin" | "user";
  status: "active" | "inactive";
  two_factor_enabled?: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface LoginData {
  requires_two_factor?: boolean;
  username?: string;
  [key: string]: unknown;
}
