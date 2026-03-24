const API_BASE = '';

let token = localStorage.getItem('token');

function showToast(message, type = 'success') {
    const toast = document.getElementById('toast');
    toast.textContent = message;
    toast.className = `toast ${type} show`;
    setTimeout(() => toast.className = 'toast', 3000);
}

function showView(viewId) {
    document.querySelectorAll('.view').forEach(v => v.style.display = 'none');
    document.getElementById(viewId).style.display = 'block';
}

function updateNav() {
    const isLoggedIn = !!token;
    document.getElementById('nav-dashboard').style.display = isLoggedIn ? 'block' : 'none';
    document.getElementById('nav-logout').style.display = isLoggedIn ? 'block' : 'none';
    document.getElementById('nav-login').style.display = isLoggedIn ? 'none' : 'block';
    document.getElementById('nav-register').style.display = isLoggedIn ? 'none' : 'block';
}

async function apiCall(endpoint, options = {}) {
    const headers = {
        'Content-Type': 'application/json',
        ...options.headers
    };
    
    if (token) {
        headers['Authorization'] = `Bearer ${token}`;
    }

    const response = await fetch(`${API_BASE}${endpoint}`, {
        ...options,
        headers
    });

    if (response.status === 401) {
        token = null;
        localStorage.removeItem('token');
        updateNav();
        showView('auth-view');
        showToast('Session expired', 'error');
        throw new Error('Unauthorized');
    }

    return response;
}

async function handleAuth(endpoint, data) {
    const response = await apiCall(endpoint, {
        method: 'POST',
        body: JSON.stringify(data)
    });

    if (!response.ok) {
        const error = await response.json();
        throw new Error(error.message || 'Request failed');
    }

    return response.json();
}

document.getElementById('nav-logout').addEventListener('click', (e) => {
    e.preventDefault();
    token = null;
    localStorage.removeItem('token');
    updateNav();
    showView('auth-view');
});

document.querySelectorAll('.tab').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
        tab.classList.add('active');
        
        const isLogin = tab.dataset.tab === 'login';
        document.getElementById('login-form').style.display = isLogin ? 'flex' : 'none';
        document.getElementById('register-form').style.display = isLogin ? 'none' : 'flex';
        document.getElementById('login-error').textContent = '';
        document.getElementById('register-error').textContent = '';
    });
});

document.getElementById('login-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const email = document.getElementById('login-email').value;
    const password = document.getElementById('login-password').value;
    const errorEl = document.getElementById('login-error');
    
    try {
        const data = await handleAuth('/api/auth/login', { email, password });
        token = data.token;
        localStorage.setItem('token', token);
        updateNav();
        showToast('Login successful!');
        showView('dashboard-view');
        loadUrls();
    } catch (err) {
        errorEl.textContent = err.message;
    }
});

document.getElementById('register-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const email = document.getElementById('register-email').value;
    const password = document.getElementById('register-password').value;
    const errorEl = document.getElementById('register-error');
    
    try {
        const data = await handleAuth('/api/auth/register', { email, password });
        token = data.token;
        localStorage.setItem('token', token);
        updateNav();
        showToast('Account created!');
        showView('dashboard-view');
        loadUrls();
    } catch (err) {
        errorEl.textContent = err.message;
    }
});

document.getElementById('nav-dashboard').addEventListener('click', (e) => {
    e.preventDefault();
    showView('dashboard-view');
    loadUrls();
});

document.getElementById('nav-login').addEventListener('click', (e) => {
    e.preventDefault();
    showView('auth-view');
});

document.getElementById('nav-register').addEventListener('click', (e) => {
    e.preventDefault();
    showView('auth-view');
});

async function loadUrls() {
    const container = document.getElementById('urls-list');
    container.innerHTML = '<div class="loading">Loading your links...</div>';
    
    try {
        const response = await apiCall('/api/urls');
        
        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.message || 'Failed to load URLs');
        }
        
        const urls = await response.json();
        
        if (urls.length === 0) {
            container.innerHTML = '<div class="empty">No links yet. Create your first short URL above!</div>';
            return;
        }
        
        container.innerHTML = urls.map(url => `
            <div class="url-card" data-id="${url.id}">
                <div class="url-info">
                    <a href="/s/${url.short_code}" target="_blank" class="short-url">${window.location.origin}/s/${url.short_code}</a>
                    <div class="original-url">${url.original_url}</div>
                    <div class="meta">
                        Created: ${new Date(url.created_at).toLocaleDateString()}
                        ${url.expires_at ? ` • Expires: ${new Date(url.expires_at).toLocaleDateString()}` : ''}
                    </div>
                </div>
                <div class="url-actions">
                    <span class="click-count">${url.click_count || 0} clicks</span>
                    <button class="btn btn-danger btn-small" onclick="deleteUrl('${url.id}')">Delete</button>
                </div>
            </div>
        `).join('');
        
    } catch (err) {
        container.innerHTML = `<div class="error">${err.message}</div>`;
    }
}

document.getElementById('create-url-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const originalUrl = document.getElementById('original-url').value;
    const shortCode = document.getElementById('short-code').value;
    const errorEl = document.getElementById('create-url-error');
    
    const payload = { original_url: originalUrl };
    if (shortCode) payload.short_code = shortCode;
    
    try {
        const response = await apiCall('/api/urls', {
            method: 'POST',
            body: JSON.stringify(payload)
        });
        
        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.message || 'Failed to create URL');
        }
        
        document.getElementById('original-url').value = '';
        document.getElementById('short-code').value = '';
        errorEl.textContent = '';
        
        showToast('URL created!');
        loadUrls();
    } catch (err) {
        errorEl.textContent = err.message;
    }
});

window.deleteUrl = async function(id) {
    if (!confirm('Are you sure you want to delete this URL?')) return;
    
    try {
        const response = await apiCall(`/api/urls/${id}`, {
            method: 'DELETE'
        });
        
        if (!response.ok) {
            const error = await response.json();
            throw new Error(error.message || 'Failed to delete URL');
        }
        
        showToast('URL deleted');
        loadUrls();
    } catch (err) {
        showToast(err.message, 'error');
    }
};

if (token) {
    updateNav();
    showView('dashboard-view');
    loadUrls();
} else {
    updateNav();
    showView('auth-view');
}
