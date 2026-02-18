terraform {
  required_version = ">= 1.14"

  cloud {
    organization = "y-junctions"

    workspaces {
      name = "y-junctions-prod"
    }
  }

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 6.0"
    }
    neon = {
      source  = "kislerdm/neon"
      version = "~> 0.6"
    }
    cockroach = {
      source  = "cockroachdb/cockroach"
      version = "~> 1.0"
    }
  }
}

provider "google" {
  project = var.project_id
  region  = var.region
}

provider "neon" {
  api_key = var.neon_api_key
}

provider "cockroach" {
  apikey = var.cockroachdb_api_key
}
