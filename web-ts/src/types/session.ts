export interface User {
  id: string;
  username: string;
  email: string;
  role: "admin" | "user";
  status: boolean;
  two_factor_enabled?: boolean;
  two_factor_verified?: boolean;
  created_at?: string;
  updated_at?: string;
}

export interface LoginData {
  user: User;
  expires_in?: number;
  requires_two_factor?: boolean;
  username?: string;
  [key: string]: unknown;
}
