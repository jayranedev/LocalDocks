# LocalDocks

> A minimal, developer-focused dashboard for managing everything running on your local machine.

LocalDocks is an open-source desktop application designed to give developers a clean and focused view of the services, processes, and ports running on their local machine.

Instead of searching through Task Manager or repeatedly using terminal commands to find which process is occupying a port, LocalDocks brings the information developers actually care about into one place.

## Status

🚧 LocalDocks is currently under active development.

The project is being developed in stages, with the initial releases focused on Windows.

## Vision

LocalDocks aims to become a lightweight control center for local development environments.

The long-term goal is to make it easy to:

- See what is running locally
- Identify which process is using a port
- Monitor resource usage
- Start, stop, restart, and terminate development services
- Group services by project
- Work with Docker and WSL
- View service logs
- Detect development environment problems
- Quickly perform common developer actions

LocalDocks is intentionally designed to remain minimal and developer-focused rather than becoming another general-purpose system task manager.

## Initial Features

The first version of LocalDocks will focus on:

- Local development processes
- Listening ports
- Process and port relationships
- CPU and memory usage
- Process details
- Searching and filtering
- Opening local services in a browser
- Copying local service URLs
- Terminating processes
- Live updates

Additional functionality will be introduced gradually.

## Roadmap

### V1 — Local Visibility

- [ ] Detect relevant local processes
- [ ] Detect listening ports
- [ ] Map ports to processes
- [ ] Display CPU usage
- [ ] Display memory usage
- [ ] Search and filter
- [ ] Process details
- [ ] Port details
- [ ] Open local services in browser
- [ ] Copy local URLs
- [ ] Terminate processes
- [ ] Live updates
- [ ] Minimal desktop interface

### V2 — Developer Control

- [ ] Start services
- [ ] Stop services
- [ ] Restart services
- [ ] Process trees
- [ ] Project detection
- [ ] Group services by project
- [ ] Detect frameworks and runtimes
- [ ] Open project directory
- [ ] Open terminal
- [ ] Resource graphs
- [ ] Port conflict assistance

### Future

- [ ] Logs
- [ ] Docker integration
- [ ] WSL integration
- [ ] Database/service detection
- [ ] Notifications
- [ ] Command palette
- [ ] Keyboard shortcuts
- [ ] Integrations
- [ ] Plugin system
- [ ] Additional platforms

The roadmap is subject to change as the project develops.

## Why LocalDocks?

Developers frequently run multiple services at the same time:

- Frontends
- APIs
- Workers
- Databases
- Development servers
- Background services

This often results in multiple ports and processes being active simultaneously.

When something goes wrong, developers typically end up switching between terminal commands, Task Manager, browser tabs, and project directories to figure out what is happening.

LocalDocks aims to make that process simpler.

## Philosophy

LocalDocks follows a few principles:

**Minimal**  
Show developers what they need without overwhelming them with unrelated system information.

**Local-first**  
The application is designed around the local development environment.

**Developer-focused**  
Features should solve problems developers actually encounter while building and testing software.

**Fast**  
The application should remain lightweight and responsive.

**Open source**  
LocalDocks is intended to become an open-source project that developers can use, inspect, improve, and contribute to.

## Contributing

Contributions will be welcomed once the project reaches its public open-source release.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## Security

For security-related issues, please see [SECURITY.md](SECURITY.md).

## License

LocalDocks is licensed under the MIT License.

See [LICENSE](LICENSE) for details.

---

Built by [Silent Minds](https://github.com/Silent-Minds).