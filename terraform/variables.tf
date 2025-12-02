variable "project_id" {
  description = "Google Cloud Project ID"
  type        = string
  default     = "y-junctions-prod"
}

variable "region" {
  description = "Google Cloud region"
  type        = string
  default     = "asia-northeast1"
}

variable "neon_api_key" {
  description = "Neon API key"
  type        = string
  sensitive   = true
}

variable "backend_image" {
  description = "Backend Docker image URL"
  type        = string
  default     = "asia-northeast1-docker.pkg.dev/y-junctions-prod/y-junction/backend:latest"
}

variable "backend_service_name" {
  description = "Cloud Run service name for backend"
  type        = string
  default     = "y-junction-api"
}

variable "frontend_bucket_name" {
  description = "Cloud Storage bucket name for frontend"
  type        = string
  default     = "y-junctions-prod-frontend"
}
