CREATE TABLE IF NOT EXISTS articles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    description TEXT,
    url TEXT NOT NULL,
    category TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Données de test
INSERT INTO articles (title, description, url, category) VALUES
('Rust Programming Language', 'A systems programming language that runs blazingly fast', 'https://www.rust-lang.org', 'programming'),
('Python for Data Science', 'Python is great for data analysis and machine learning', 'https://python.org', 'data-science'),
('Introduction to Docker', 'Containerization platform for developers', 'https://docker.com', 'devops'),
('JavaScript ES6 Features', 'Modern JavaScript features and syntax', 'https://developer.mozilla.org', 'web'),
('Machine Learning Basics', 'Introduction to machine learning algorithms', 'https://ml-course.com', 'ai'),
('Web Security Best Practices', 'How to secure your web applications', 'https://owasp.org', 'security'),
('Galaxy Journal Project', 'A space exploration documentation project', 'https://galaxy-journal.space', 'space'),
('Black Hole Physics', 'Understanding black holes and event horizons', 'https://nasa.gov', 'physics'),
('Mobile App Development', 'Building apps for iOS and Android', 'https://flutter.dev', 'mobile'),
('Cloud Computing with AWS', 'Introduction to Amazon Web Services', 'https://aws.amazon.com', 'cloud');